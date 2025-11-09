use futures::{SinkExt, StreamExt, future};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio_util::codec::Framed;

use crate::codec::{StompCodec, StompItem};
use crate::frame::Frame;

/// Errors returned by `Connection` operations.
#[derive(Error, Debug)]
pub enum ConnError {
    /// I/O-level error
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Protocol-level error
    #[error("protocol error: {0}")]
    Protocol(String),
}

/// Parse the STOMP `heart-beat` header value (format: "cx,cy").
///
/// Parameters
/// - `header`: header string from the server or client (for example
///   "10000,10000"). The values represent milliseconds.
///
/// Returns a tuple `(cx, cy)` where each value is the heartbeat interval in
/// milliseconds. Missing or invalid fields default to `0`.
pub fn parse_heartbeat_header(header: &str) -> (u64, u64) {
    let mut parts = header.split(',');
    let cx = parts
        .next()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let cy = parts
        .next()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    (cx, cy)
}

/// Negotiate heartbeat intervals between client and server.
///
/// Parameters
/// - `client_out`: client's desired outgoing heartbeat interval in
///   milliseconds (how often the client will send heartbeats).
/// - `client_in`: client's desired incoming heartbeat interval in
///   milliseconds (how often the client expects to receive heartbeats).
/// - `server_out`: server's advertised outgoing interval in milliseconds.
/// - `server_in`: server's advertised incoming interval in milliseconds.
///
/// Returns `(outgoing, incoming)` where each element is `Some(Duration)` if
/// heartbeats are enabled in that direction, or `None` if disabled. The
/// negotiated interval uses the STOMP rule of taking the maximum of the
/// corresponding client and server values.
pub fn negotiate_heartbeats(
    client_out: u64,
    client_in: u64,
    server_out: u64,
    server_in: u64,
) -> (Option<Duration>, Option<Duration>) {
    let negotiated_out_ms = std::cmp::max(client_out, server_in);
    let negotiated_in_ms = std::cmp::max(client_in, server_out);

    let outgoing = if negotiated_out_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(negotiated_out_ms))
    };
    let incoming = if negotiated_in_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(negotiated_in_ms))
    };
    (outgoing, incoming)
}

/// High-level connection object that manages a single TCP/STOMP connection.
///
/// The `Connection` spawns a background task that maintains the TCP transport,
/// sends/receives STOMP frames using `StompCodec`, negotiates heartbeats, and
/// performs simple reconnect logic with exponential backoff.
pub struct Connection {
    outbound_tx: mpsc::Sender<StompItem>,
    inbound_rx: mpsc::Receiver<Frame>,
    shutdown_tx: broadcast::Sender<()>,
}

impl Connection {
    /// Establish a connection to the STOMP server at `addr` with the given
    /// credentials and heartbeat header string (e.g. "10000,10000").
    ///
    /// Parameters
    /// - `addr`: TCP address (host:port) of the STOMP server.
    /// - `login`: login username for STOMP `CONNECT`.
    /// - `passcode`: passcode for STOMP `CONNECT`.
    /// - `client_hb`: client's `heart-beat` header value ("cx,cy" in
    ///   milliseconds) that will be sent in the `CONNECT` frame.
    ///
    /// Returns a `Connection` which provides `send_frame`, `next_frame`, and
    /// `close` helpers. The detailed connection handling (I/O, heartbeats,
    /// reconnects) runs on a background task spawned by this method.
    pub async fn connect(
        addr: &str,
        login: &str,
        passcode: &str,
        client_hb: &str,
    ) -> Result<Self, ConnError> {
        let (out_tx, mut out_rx) = mpsc::channel::<StompItem>(32);
        let (in_tx, in_rx) = mpsc::channel::<Frame>(32);
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let addr = addr.to_string();
        let login = login.to_string();
        let passcode = passcode.to_string();
        let client_hb = client_hb.to_string();

        let shutdown_tx_clone = shutdown_tx.clone();

        tokio::spawn(async move {
            let mut backoff_secs: u64 = 1;
            loop {
                let mut shutdown_sub = shutdown_tx_clone.subscribe();

                tokio::select! {
                    _ = shutdown_sub.recv() => break,
                    _ = future::ready(()) => {},
                }

                match TcpStream::connect(&addr).await {
                    Ok(stream) => {
                        let mut framed = Framed::new(stream, StompCodec::new());

                        let mut connect = Frame::new("CONNECT");
                        connect = connect.header("accept-version", "1.2");
                        connect = connect.header("host", "/");
                        connect = connect.header("login", &login);
                        connect = connect.header("passcode", &passcode);
                        connect = connect.header("heart-beat", &client_hb);

                        if framed.send(StompItem::Frame(connect)).await.is_err() {
                            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                            backoff_secs = (backoff_secs * 2).min(30);
                            continue;
                        }

                        let mut server_heartbeat = "0,0".to_string();
                        loop {
                            tokio::select! {
                                _ = shutdown_sub.recv() => break,
                                item = framed.next() => {
                                    match item {
                                        Some(Ok(StompItem::Heartbeat)) => {}
                                        Some(Ok(StompItem::Frame(f))) => {
                                            if f.command == "CONNECTED" {
                                                for (k, v) in &f.headers {
                                                    if k.to_lowercase() == "heart-beat" { server_heartbeat = v.clone(); }
                                                }
                                                break;
                                            }
                                        }
                                        _ => break,
                                    }
                                }
                            }
                        }

                        let (cx, cy) = parse_heartbeat_header(&client_hb);
                        let (sx, sy) = parse_heartbeat_header(&server_heartbeat);
                        let (send_interval, recv_interval) = negotiate_heartbeats(cx, cy, sx, sy);

                        let last_received = Arc::new(AtomicU64::new(current_millis()));
                        let writer_last_sent = Arc::new(AtomicU64::new(current_millis()));

                        let (mut sink, mut stream) = framed.split();
                        let in_tx = in_tx.clone();

                        let mut hb_tick = match send_interval {
                            Some(d) => tokio::time::interval(d),
                            None => tokio::time::interval(Duration::from_secs(86400)),
                        };
                        let watchdog_half = recv_interval.map(|d| d / 2);

                        backoff_secs = 1;

                        'conn: loop {
                            tokio::select! {
                                _ = shutdown_sub.recv() => { let _ = sink.close().await; break 'conn; }
                                maybe = out_rx.recv() => {
                                    match maybe {
                                        Some(item) => if sink.send(item).await.is_err() { break 'conn } else { writer_last_sent.store(current_millis(), Ordering::SeqCst); }
                                        None => break 'conn,
                                    }
                                }
                                item = stream.next() => {
                                    match item {
                                        Some(Ok(StompItem::Heartbeat)) => { last_received.store(current_millis(), Ordering::SeqCst); }
                                        Some(Ok(StompItem::Frame(f))) => { last_received.store(current_millis(), Ordering::SeqCst); let _ = in_tx.send(f).await; }
                                        Some(Err(_)) | None => break 'conn,
                                    }
                                }
                                _ = hb_tick.tick() => {
                                    if let Some(dur) = send_interval {
                                        let last = writer_last_sent.load(Ordering::SeqCst);
                                        if current_millis().saturating_sub(last) >= dur.as_millis() as u64 {
                                            if sink.send(StompItem::Heartbeat).await.is_err() { break 'conn; }
                                            writer_last_sent.store(current_millis(), Ordering::SeqCst);
                                        }
                                    }
                                }
                                _ = async { if let Some(interval) = watchdog_half { tokio::time::sleep(interval).await } else { future::pending::<()>().await } } => {
                                    if let Some(recv_dur) = recv_interval {
                                        let last = last_received.load(Ordering::SeqCst);
                                        if current_millis().saturating_sub(last) > (recv_dur.as_millis() as u64 * 2) {
                                            let _ = sink.close().await; break 'conn;
                                        }
                                    }
                                }
                            }
                        }

                        if shutdown_sub.try_recv().is_ok() {
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(30);
                    }
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(30);
                    }
                }
            }
        });

        Ok(Connection {
            outbound_tx: out_tx,
            inbound_rx: in_rx,
            shutdown_tx,
        })
    }

    pub async fn send_frame(&self, frame: Frame) -> Result<(), ConnError> {
        // Send a frame to the background writer task.
        //
        // Parameters
        // - `frame`: ownership of the `Frame` to send. The frame is converted
        //   into a `StompItem::Frame` and sent over the internal mpsc channel.
        self.outbound_tx
            .send(StompItem::Frame(frame))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))
    }

    pub async fn next_frame(&mut self) -> Option<Frame> {
        // Receive the next inbound `Frame` produced by the background reader
        // task. Returns `Some(Frame)` when available or `None` if the inbound
        // channel has been closed.
        self.inbound_rx.recv().await
    }

    pub async fn close(self) {
        // Signal the background task to shutdown by broadcasting on the
        // shutdown channel. Consumers may await task termination separately
        // if needed.
        let _ = self.shutdown_tx.send(());
    }
}

fn current_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
