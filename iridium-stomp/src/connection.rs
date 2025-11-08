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

#[derive(Error, Debug)]
pub enum ConnError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(String),
}

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

pub struct Connection {
    outbound_tx: mpsc::Sender<StompItem>,
    inbound_rx: mpsc::Receiver<Frame>,
    shutdown_tx: broadcast::Sender<()>,
}

impl Connection {
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
        self.outbound_tx
            .send(StompItem::Frame(frame))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))
    }

    pub async fn next_frame(&mut self) -> Option<Frame> {
        self.inbound_rx.recv().await
    }

    pub async fn close(self) {
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
