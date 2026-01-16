use futures::{SinkExt, StreamExt, future};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tokio_util::codec::Framed;

use crate::codec::{StompCodec, StompItem};
use crate::frame::Frame;

/// Internal subscription entry stored for each destination.
#[derive(Clone)]
pub(crate) struct SubscriptionEntry {
    pub(crate) id: String,
    pub(crate) sender: mpsc::Sender<Frame>,
    pub(crate) ack: String,
    pub(crate) headers: Vec<(String, String)>,
}

/// Alias for the subscription dispatch map: destination -> list of
/// `SubscriptionEntry`.
pub(crate) type Subscriptions = HashMap<String, Vec<SubscriptionEntry>>;

/// Alias for the pending map: subscription_id -> queue of (message-id, Frame).
pub(crate) type PendingMap = HashMap<String, VecDeque<(String, Frame)>>;

/// Internal type for resubscribe snapshot entries: (destination, id, ack, headers)
pub(crate) type ResubEntry = (String, String, String, Vec<(String, String)>);

/// Alias for pending receipt map: receipt-id -> oneshot sender to notify when received.
pub(crate) type PendingReceipts = HashMap<String, oneshot::Sender<()>>;

/// Errors returned by `Connection` operations.
#[derive(Error, Debug)]
pub enum ConnError {
    /// I/O-level error
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Protocol-level error
    #[error("protocol error: {0}")]
    Protocol(String),
    /// Receipt timeout error
    #[error("receipt timeout: no RECEIPT received for '{0}' within timeout")]
    ReceiptTimeout(String),
}

/// Subscription acknowledgement modes as defined by STOMP 1.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckMode {
    Auto,
    Client,
    ClientIndividual,
}

impl AckMode {
    fn as_str(&self) -> &'static str {
        match self {
            AckMode::Auto => "auto",
            AckMode::Client => "client",
            AckMode::ClientIndividual => "client-individual",
        }
    }
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
#[derive(Clone)]
pub struct Connection {
    outbound_tx: mpsc::Sender<StompItem>,
    /// The inbound receiver is shared behind a mutex so the `Connection`
    /// handle may be cloned and callers can call `next_frame` concurrently.
    inbound_rx: Arc<Mutex<mpsc::Receiver<Frame>>>,
    shutdown_tx: broadcast::Sender<()>,
    /// Map of destination -> list of (subscription id, sender) for dispatching
    /// inbound MESSAGE frames to subscribers.
    subscriptions: Arc<Mutex<Subscriptions>>,
    /// Monotonic counter used to allocate subscription ids.
    sub_id_counter: Arc<AtomicU64>,
    /// Pending messages awaiting ACK/NACK from the application.
    ///
    /// Organized by subscription id. For `client` ack mode the ACK is
    /// cumulative: acknowledging message `M` for subscription `S` acknowledges
    /// all messages previously delivered for `S` up to and including `M`.
    /// For `client-individual` the ACK/NACK applies only to the single
    /// message.
    pending: Arc<Mutex<PendingMap>>,
    /// Pending receipt confirmations.
    ///
    /// When a frame is sent with a `receipt` header, the receipt-id is stored
    /// here with a oneshot sender. When the server responds with a RECEIPT
    /// frame, the sender is notified.
    pending_receipts: Arc<Mutex<PendingReceipts>>,
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
        let subscriptions: Arc<Mutex<Subscriptions>> = Arc::new(Mutex::new(HashMap::new()));
        let sub_id_counter = Arc::new(AtomicU64::new(1));
        let (shutdown_tx, _) = broadcast::channel::<()>(1);
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();
        let pending_receipts: Arc<Mutex<PendingReceipts>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_receipts_clone = pending_receipts.clone();

        let addr = addr.to_string();
        let login = login.to_string();
        let passcode = passcode.to_string();
        let client_hb = client_hb.to_string();

        let shutdown_tx_clone = shutdown_tx.clone();
        let subscriptions_clone = subscriptions.clone();

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
                        let subscriptions = subscriptions_clone.clone();

                        // Clear pending message map on reconnect — messages that were
                        // outstanding before the disconnect are considered lost and
                        // will be redelivered by the server as appropriate.
                        {
                            let mut p = pending_clone.lock().await;
                            p.clear();
                        }

                        // Resubscribe any existing subscriptions after reconnect.
                        // We snapshot the subscription entries while holding the lock
                        // and then issue SUBSCRIBE frames using the sink.
                        let subs_snapshot: Vec<ResubEntry> = {
                            let map = subscriptions.lock().await;
                            let mut v: Vec<ResubEntry> = Vec::new();
                            for (dest, vec) in map.iter() {
                                for entry in vec.iter() {
                                    v.push((
                                        dest.clone(),
                                        entry.id.clone(),
                                        entry.ack.clone(),
                                        entry.headers.clone(),
                                    ));
                                }
                            }
                            v
                        };

                        for (dest, id, ack, headers) in subs_snapshot {
                            let mut sf = Frame::new("SUBSCRIBE");
                            sf = sf
                                .header("id", &id)
                                .header("destination", &dest)
                                .header("ack", &ack);
                            for (k, v) in headers {
                                sf = sf.header(&k, &v);
                            }
                            let _ = sink.send(StompItem::Frame(sf)).await;
                        }

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
                                        Some(Ok(StompItem::Frame(f))) => {
                                            last_received.store(current_millis(), Ordering::SeqCst);
                                            // Dispatch MESSAGE frames to any matching subscribers.
                                            if f.command == "MESSAGE" {
                                                // try to find destination, subscription and message-id headers
                                                let mut dest_opt: Option<String> = None;
                                                let mut sub_opt: Option<String> = None;
                                                let mut msg_id_opt: Option<String> = None;
                                                for (k, v) in &f.headers {
                                                    let kl = k.to_lowercase();
                                                    if kl == "destination" {
                                                        dest_opt = Some(v.clone());
                                                    } else if kl == "subscription" {
                                                        sub_opt = Some(v.clone());
                                                    } else if kl == "message-id" {
                                                        msg_id_opt = Some(v.clone());
                                                    }
                                                }

                                                // Determine whether we need to track this message as pending
                                                let mut need_pending = false;
                                                if let Some(sub_id) = &sub_opt {
                                                    let map = subscriptions.lock().await;
                                                    for (_dest, vec) in map.iter() {
                                                        for entry in vec.iter() {
                                                            if &entry.id == sub_id && entry.ack != "auto" {
                                                                need_pending = true;
                                                            }
                                                        }
                                                    }
                                                } else if let Some(dest) = &dest_opt {
                                                    let map = subscriptions.lock().await;
                                                    if let Some(vec) = map.get(dest) {
                                                        for entry in vec.iter() {
                                                            if entry.ack != "auto" {
                                                                need_pending = true;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }

                                                // If required, add to pending map (per-subscription) before
                                                // delivery so ACK/NACK requests from the application can
                                                // reference the message. We require a `message-id` header
                                                // to track messages; if missing, we cannot support ACK/NACK.
                                                if let Some(msg_id) = msg_id_opt.clone().filter(|_| need_pending) {
                                                    // If the server provided a subscription id in the
                                                    // MESSAGE, store pending under that subscription.
                                                    if let Some(sub_id) = &sub_opt {
                                                        let mut p = pending_clone.lock().await;
                                                        let q = p
                                                            .entry(sub_id.clone())
                                                            .or_insert_with(VecDeque::new);
                                                        q.push_back((msg_id.clone(), f.clone()));
                                                    } else if let Some(dest) = &dest_opt {
                                                        // Destination-based delivery: add the message to
                                                        // the pending queue for each matching
                                                        // subscription on that destination.
                                                        let map = subscriptions.lock().await;
                                                        if let Some(vec) = map.get(dest) {
                                                            let mut p = pending_clone.lock().await;
                                                            for entry in vec.iter() {
                                                                let q = p
                                                                    .entry(entry.id.clone())
                                                                    .or_insert_with(VecDeque::new);
                                                                q.push_back((msg_id.clone(), f.clone()));
                                                            }
                                                        }
                                                    }
                                                }

                                                // Deliver to subscribers.
                                                if let Some(sub_id) = sub_opt {
                                                    let mut map = subscriptions.lock().await;
                                                    for (_dest, vec) in map.iter_mut() {
                                                        vec.retain(|entry| {
                                                            if entry.id == sub_id {
                                                                let _ = entry.sender.try_send(f.clone());
                                                                true
                                                            } else {
                                                                true
                                                            }
                                                        });
                                                    }
                                                } else if let Some(dest) = dest_opt {
                                                    let mut map = subscriptions.lock().await;
                                                    if let Some(vec) = map.get_mut(&dest) {
                                                        vec.retain(|entry| entry.sender.try_send(f.clone()).is_ok());
                                                    }
                                                }
                                            } else if f.command == "RECEIPT" {
                                                // Handle RECEIPT frame: notify any waiting callers
                                                if let Some(receipt_id) = f.get_header("receipt-id") {
                                                    let mut receipts = pending_receipts_clone.lock().await;
                                                    if let Some(sender) = receipts.remove(receipt_id) {
                                                        let _ = sender.send(());
                                                    }
                                                }
                                                // Don't forward RECEIPT frames to inbound channel
                                                continue;
                                            }

                                            let _ = in_tx.send(f).await;
                                        }
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
            inbound_rx: Arc::new(Mutex::new(in_rx)),
            shutdown_tx,
            subscriptions,
            sub_id_counter,
            pending,
            pending_receipts,
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

    /// Send a frame with a receipt request and return the receipt ID.
    ///
    /// This method adds a unique `receipt` header to the frame and registers
    /// the receipt ID for tracking. Use `wait_for_receipt()` to wait for the
    /// server's RECEIPT response.
    ///
    /// # Parameters
    /// - `frame`: the frame to send. A `receipt` header will be added.
    ///
    /// # Returns
    /// The generated receipt ID that can be used with `wait_for_receipt()`.
    ///
    /// # Example
    /// ```ignore
    /// let receipt_id = conn.send_frame_with_receipt(frame).await?;
    /// conn.wait_for_receipt(&receipt_id, Duration::from_secs(5)).await?;
    /// ```
    pub async fn send_frame_with_receipt(&self, frame: Frame) -> Result<String, ConnError> {
        use std::sync::atomic::AtomicU64;
        use std::sync::atomic::Ordering::SeqCst;

        // Generate a unique receipt ID using a static counter
        static RECEIPT_COUNTER: AtomicU64 = AtomicU64::new(1);
        let receipt_id = format!("rcpt-{}", RECEIPT_COUNTER.fetch_add(1, SeqCst));

        // Create the oneshot channel for notification
        let (tx, _rx) = oneshot::channel();

        // Register the pending receipt
        {
            let mut receipts = self.pending_receipts.lock().await;
            receipts.insert(receipt_id.clone(), tx);
        }

        // Add receipt header and send the frame
        let frame_with_receipt = frame.receipt(&receipt_id);
        self.send_frame(frame_with_receipt).await?;

        Ok(receipt_id)
    }

    /// Wait for a receipt confirmation from the server.
    ///
    /// This method blocks until the server sends a RECEIPT frame with the
    /// matching receipt-id, or until the timeout expires.
    ///
    /// # Parameters
    /// - `receipt_id`: the receipt ID returned by `send_frame_with_receipt()`.
    /// - `timeout`: maximum time to wait for the receipt.
    ///
    /// # Returns
    /// `Ok(())` if the receipt was received, or `Err(ConnError::ReceiptTimeout)`
    /// if the timeout expired.
    ///
    /// # Example
    /// ```ignore
    /// let receipt_id = conn.send_frame_with_receipt(frame).await?;
    /// conn.wait_for_receipt(&receipt_id, Duration::from_secs(5)).await?;
    /// println!("Message confirmed!");
    /// ```
    pub async fn wait_for_receipt(
        &self,
        receipt_id: &str,
        timeout: Duration,
    ) -> Result<(), ConnError> {
        // Get the receiver for this receipt
        let rx = {
            let mut receipts = self.pending_receipts.lock().await;
            // Re-create the oneshot channel and swap out the sender
            let (tx, rx) = oneshot::channel();
            if let Some(old_tx) = receipts.insert(receipt_id.to_string(), tx) {
                // Drop the old sender - this is expected if called after send_frame_with_receipt
                drop(old_tx);
            }
            rx
        };

        // Wait for the receipt with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => {
                // Channel was closed without receiving - connection likely dropped
                Err(ConnError::Protocol(
                    "receipt channel closed unexpectedly".into(),
                ))
            }
            Err(_) => {
                // Timeout expired - clean up the pending receipt
                let mut receipts = self.pending_receipts.lock().await;
                receipts.remove(receipt_id);
                Err(ConnError::ReceiptTimeout(receipt_id.to_string()))
            }
        }
    }

    /// Send a frame and wait for server confirmation via RECEIPT.
    ///
    /// This is a convenience method that combines `send_frame_with_receipt()`
    /// and `wait_for_receipt()`. Use this when you want to ensure a frame
    /// was processed by the server before continuing.
    ///
    /// # Parameters
    /// - `frame`: the frame to send.
    /// - `timeout`: maximum time to wait for the receipt.
    ///
    /// # Returns
    /// `Ok(())` if the frame was sent and receipt confirmed, or an error if
    /// sending failed or the receipt timed out.
    ///
    /// # Example
    /// ```ignore
    /// let frame = Frame::new("SEND")
    ///     .header("destination", "/queue/orders")
    ///     .set_body(b"order data".to_vec());
    ///
    /// conn.send_frame_confirmed(frame, Duration::from_secs(5)).await?;
    /// println!("Order sent and confirmed!");
    /// ```
    pub async fn send_frame_confirmed(
        &self,
        frame: Frame,
        timeout: Duration,
    ) -> Result<(), ConnError> {
        // Generate receipt ID and register before sending
        use std::sync::atomic::AtomicU64;
        use std::sync::atomic::Ordering::SeqCst;

        static RECEIPT_COUNTER: AtomicU64 = AtomicU64::new(1);
        let receipt_id = format!("rcpt-{}", RECEIPT_COUNTER.fetch_add(1, SeqCst));

        // Create the oneshot channel for notification
        let (tx, rx) = oneshot::channel();

        // Register the pending receipt before sending
        {
            let mut receipts = self.pending_receipts.lock().await;
            receipts.insert(receipt_id.clone(), tx);
        }

        // Add receipt header and send the frame
        let frame_with_receipt = frame.receipt(&receipt_id);
        self.send_frame(frame_with_receipt).await?;

        // Wait for the receipt with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(ConnError::Protocol(
                "receipt channel closed unexpectedly".into(),
            )),
            Err(_) => {
                // Timeout expired - clean up
                let mut receipts = self.pending_receipts.lock().await;
                receipts.remove(&receipt_id);
                Err(ConnError::ReceiptTimeout(receipt_id))
            }
        }
    }

    /// Subscribe to a destination.
    ///
    /// Parameters
    /// - `destination`: the STOMP destination to subscribe to (e.g. "/queue/foo").
    /// - `ack`: acknowledgement mode to request from the server.
    ///
    /// Returns a tuple `(subscription_id, receiver)` where `subscription_id` is
    /// the opaque id assigned locally for this subscription and `receiver` is a
    /// `mpsc::Receiver<Frame>` which will yield incoming MESSAGE frames for the
    /// destination. The caller should read from the receiver to handle messages.
    /// Subscribe to a destination using optional extra headers.
    ///
    /// This variant accepts additional headers which are stored locally and
    /// re-sent on reconnect. Use `subscribe` as a convenience wrapper when no
    /// extra headers are needed.
    pub async fn subscribe_with_headers(
        &self,
        destination: &str,
        ack: AckMode,
        extra_headers: Vec<(String, String)>,
    ) -> Result<crate::subscription::Subscription, ConnError> {
        let id = self
            .sub_id_counter
            .fetch_add(1, Ordering::SeqCst)
            .to_string();
        let (tx, rx) = mpsc::channel::<Frame>(16);
        {
            let mut map = self.subscriptions.lock().await;
            map.entry(destination.to_string())
                .or_insert_with(Vec::new)
                .push(SubscriptionEntry {
                    id: id.clone(),
                    sender: tx.clone(),
                    ack: ack.as_str().to_string(),
                    headers: extra_headers.clone(),
                });
        }

        let mut f = Frame::new("SUBSCRIBE");
        f = f
            .header("id", &id)
            .header("destination", destination)
            .header("ack", ack.as_str());
        for (k, v) in &extra_headers {
            f = f.header(k, v);
        }
        self.outbound_tx
            .send(StompItem::Frame(f))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))?;

        Ok(crate::subscription::Subscription::new(
            id,
            destination.to_string(),
            rx,
            self.clone(),
        ))
    }

    /// Convenience wrapper without extra headers.
    pub async fn subscribe(
        &self,
        destination: &str,
        ack: AckMode,
    ) -> Result<crate::subscription::Subscription, ConnError> {
        self.subscribe_with_headers(destination, ack, Vec::new())
            .await
    }

    /// Subscribe with a typed `SubscriptionOptions` structure.
    ///
    /// `SubscriptionOptions.headers` are forwarded to the broker and persisted
    /// for automatic resubscribe after reconnect. If `durable_queue` is set,
    /// it will be used as the actual destination instead of `destination`.
    pub async fn subscribe_with_options(
        &self,
        destination: &str,
        ack: AckMode,
        options: crate::subscription::SubscriptionOptions,
    ) -> Result<crate::subscription::Subscription, ConnError> {
        let dest = options
            .durable_queue
            .as_deref()
            .unwrap_or(destination)
            .to_string();
        self.subscribe_with_headers(&dest, ack, options.headers)
            .await
    }

    /// Unsubscribe a previously created subscription by its local subscription id.
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<(), ConnError> {
        let mut found = false;
        {
            let mut map = self.subscriptions.lock().await;
            let mut remove_keys: Vec<String> = Vec::new();
            for (dest, vec) in map.iter_mut() {
                if let Some(pos) = vec.iter().position(|entry| entry.id == subscription_id) {
                    vec.remove(pos);
                    found = true;
                }
                if vec.is_empty() {
                    remove_keys.push(dest.clone());
                }
            }
            for k in remove_keys {
                map.remove(&k);
            }
        }

        if !found {
            return Err(ConnError::Protocol("subscription id not found".into()));
        }

        let mut f = Frame::new("UNSUBSCRIBE");
        f = f.header("id", subscription_id);
        self.outbound_tx
            .send(StompItem::Frame(f))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))?;

        Ok(())
    }

    /// Acknowledge a message previously received in `client` or
    /// `client-individual` ack modes.
    ///
    /// STOMP ack semantics:
    /// - `auto`: server considers message delivered immediately; the client
    ///   should not ack.
    /// - `client`: cumulative acknowledgements. ACKing message `M` for
    ///   subscription `S` acknowledges all messages delivered to `S` up to
    ///   and including `M`.
    /// - `client-individual`: only the named message is acknowledged.
    ///
    /// Parameters
    /// - `subscription_id`: the local subscription id returned by
    ///   `Connection::subscribe`. This disambiguates which subscription's
    ///   pending queue to advance for cumulative ACKs.
    /// - `message_id`: the `message-id` header value from the received
    ///   MESSAGE frame to acknowledge.
    ///
    /// Behavior
    /// - The pending queue for `subscription_id` is searched for `message_id`.
    ///   If the subscription used `client` ack mode, all pending messages up to
    ///   and including the matched message are removed. If the subscription
    ///   used `client-individual`, only the matched message is removed.
    /// - An `ACK` frame is sent to the server with `id=<message_id>` and
    ///   `subscription=<subscription_id>` headers.
    #[allow(clippy::collapsible_if, clippy::collapsible_else_if)]
    pub async fn ack(&self, subscription_id: &str, message_id: &str) -> Result<(), ConnError> {
        // Remove from the local pending queue according to subscription ack mode.
        let mut removed_any = false;
        {
            let mut p = self.pending.lock().await;
            if let Some(queue) = p.get_mut(subscription_id) {
                if let Some(pos) = queue.iter().position(|(mid, _)| mid == message_id) {
                    // Determine ack mode for this subscription (default to client).
                    let mut ack_mode = "client".to_string();
                    {
                        let map = self.subscriptions.lock().await;
                        'outer: for (_dest, vec) in map.iter() {
                            for entry in vec.iter() {
                                if entry.id == subscription_id {
                                    ack_mode = entry.ack.clone();
                                    break 'outer;
                                }
                            }
                        }
                    }

                    if ack_mode == "client" {
                        // cumulative: remove up to and including pos
                        for _ in 0..=pos {
                            queue.pop_front();
                            removed_any = true;
                        }
                    } else if queue.remove(pos).is_some() {
                        // client-individual: remove only the specific message
                        removed_any = true;
                    }

                    if queue.is_empty() {
                        p.remove(subscription_id);
                    }
                }
            }
        }

        // Send ACK to server (include subscription header for clarity)
        let mut f = Frame::new("ACK");
        f = f
            .header("id", message_id)
            .header("subscription", subscription_id);
        self.outbound_tx
            .send(StompItem::Frame(f))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))?;

        // If message wasn't found locally, still send ACK to server; server
        // may ignore or treat it as no-op.
        let _ = removed_any;
        Ok(())
    }

    /// Negative-acknowledge a message (NACK).
    ///
    /// Parameters
    /// - `subscription_id`: the local subscription id the message was delivered under.
    /// - `message_id`: the `message-id` header value from the received MESSAGE.
    ///
    /// Behavior
    /// - Removes the message from the local pending queue (cumulatively if the
    ///   subscription used `client` ack mode, otherwise only the single
    ///   message). Sends a `NACK` frame to the server with `id` and
    ///   `subscription` headers.
    #[allow(clippy::collapsible_if, clippy::collapsible_else_if)]
    pub async fn nack(&self, subscription_id: &str, message_id: &str) -> Result<(), ConnError> {
        // Mirror ack removal semantics for pending map.
        let mut removed_any = false;
        {
            let mut p = self.pending.lock().await;
            if let Some(queue) = p.get_mut(subscription_id) {
                if let Some(pos) = queue.iter().position(|(mid, _)| mid == message_id) {
                    let mut ack_mode = "client".to_string();
                    {
                        let map = self.subscriptions.lock().await;
                        'outer2: for (_dest, vec) in map.iter() {
                            for entry in vec.iter() {
                                if entry.id == subscription_id {
                                    ack_mode = entry.ack.clone();
                                    break 'outer2;
                                }
                            }
                        }
                    }

                    if ack_mode == "client" {
                        for _ in 0..=pos {
                            queue.pop_front();
                            removed_any = true;
                        }
                    } else if queue.remove(pos).is_some() {
                        removed_any = true;
                    }

                    if queue.is_empty() {
                        p.remove(subscription_id);
                    }
                }
            }
        }

        let mut f = Frame::new("NACK");
        f = f
            .header("id", message_id)
            .header("subscription", subscription_id);
        self.outbound_tx
            .send(StompItem::Frame(f))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))?;

        let _ = removed_any;
        Ok(())
    }

    /// Helper to send a transaction frame (BEGIN, COMMIT, or ABORT).
    async fn send_transaction_frame(
        &self,
        command: &str,
        transaction_id: &str,
    ) -> Result<(), ConnError> {
        let f = Frame::new(command).header("transaction", transaction_id);
        self.outbound_tx
            .send(StompItem::Frame(f))
            .await
            .map_err(|_| ConnError::Protocol("send channel closed".into()))
    }

    /// Begin a transaction.
    ///
    /// Parameters
    /// - `transaction_id`: unique identifier for the transaction. The caller is
    ///   responsible for ensuring uniqueness within the connection.
    ///
    /// Behavior
    /// - Sends a `BEGIN` frame to the server with `transaction:<transaction_id>`
    ///   header. Subsequent `SEND`, `ACK`, and `NACK` frames may include this
    ///   transaction id to group them into the transaction. The transaction must
    ///   be finalized with either `commit` or `abort`.
    pub async fn begin(&self, transaction_id: &str) -> Result<(), ConnError> {
        self.send_transaction_frame("BEGIN", transaction_id).await
    }

    /// Commit a transaction.
    ///
    /// Parameters
    /// - `transaction_id`: the transaction identifier previously passed to `begin`.
    ///
    /// Behavior
    /// - Sends a `COMMIT` frame to the server with `transaction:<transaction_id>`
    ///   header. All operations within the transaction are applied atomically.
    pub async fn commit(&self, transaction_id: &str) -> Result<(), ConnError> {
        self.send_transaction_frame("COMMIT", transaction_id).await
    }

    /// Abort a transaction.
    ///
    /// Parameters
    /// - `transaction_id`: the transaction identifier previously passed to `begin`.
    ///
    /// Behavior
    /// - Sends an `ABORT` frame to the server with `transaction:<transaction_id>`
    ///   header. All operations within the transaction are discarded.
    pub async fn abort(&self, transaction_id: &str) -> Result<(), ConnError> {
        self.send_transaction_frame("ABORT", transaction_id).await
    }

    pub async fn next_frame(&self) -> Option<Frame> {
        // Receive the next inbound `Frame` produced by the background reader
        // task. Returns `Some(Frame)` when available or `None` if the inbound
        // channel has been closed. We lock the receiver so cloned handles can
        // safely await concurrently (they serialize on the mutex).
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    // Helper to build a MESSAGE frame with given message-id and subscription/destination headers
    fn make_message(
        message_id: &str,
        subscription: Option<&str>,
        destination: Option<&str>,
    ) -> Frame {
        let mut f = Frame::new("MESSAGE");
        f = f.header("message-id", message_id);
        if let Some(s) = subscription {
            f = f.header("subscription", s);
        }
        if let Some(d) = destination {
            f = f.header("destination", d);
        }
        f
    }

    #[tokio::test]
    async fn test_cumulative_ack_removes_prefix() {
        // setup channels
        let (out_tx, mut out_rx) = mpsc::channel::<StompItem>(8);
        let (_in_tx, in_rx) = mpsc::channel::<Frame>(8);
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let subscriptions: Arc<Mutex<Subscriptions>> = Arc::new(Mutex::new(HashMap::new()));
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));

        let sub_id_counter = Arc::new(AtomicU64::new(1));

        // create a subscription entry s1 with client (cumulative) ack
        let (sub_sender, _sub_rx) = mpsc::channel::<Frame>(4);
        {
            let mut map = subscriptions.lock().await;
            map.insert(
                "/queue/x".to_string(),
                vec![SubscriptionEntry {
                    id: "s1".to_string(),
                    sender: sub_sender,
                    ack: "client".to_string(),
                    headers: Vec::new(),
                }],
            );
        }

        // fill pending queue for s1: m1,m2,m3
        {
            let mut p = pending.lock().await;
            let mut q = VecDeque::new();
            q.push_back((
                "m1".to_string(),
                make_message("m1", Some("s1"), Some("/queue/x")),
            ));
            q.push_back((
                "m2".to_string(),
                make_message("m2", Some("s1"), Some("/queue/x")),
            ));
            q.push_back((
                "m3".to_string(),
                make_message("m3", Some("s1"), Some("/queue/x")),
            ));
            p.insert("s1".to_string(), q);
        }

        let conn = Connection {
            outbound_tx: out_tx,
            inbound_rx: Arc::new(Mutex::new(in_rx)),
            shutdown_tx,
            subscriptions: subscriptions.clone(),
            sub_id_counter,
            pending: pending.clone(),
            pending_receipts: Arc::new(Mutex::new(HashMap::new())),
        };

        // ack m2 cumulatively: should remove m1 and m2, leaving m3
        conn.ack("s1", "m2").await.expect("ack failed");

        // verify pending for s1 contains only m3
        {
            let p = pending.lock().await;
            let q = p.get("s1").expect("missing s1");
            assert_eq!(q.len(), 1);
            assert_eq!(q.front().unwrap().0, "m3");
        }

        // verify an ACK frame was emitted
        if let Some(item) = out_rx.recv().await {
            match item {
                StompItem::Frame(f) => assert_eq!(f.command, "ACK"),
                _ => panic!("expected frame"),
            }
        } else {
            panic!("no outbound frame sent")
        }
    }

    #[tokio::test]
    async fn test_client_individual_ack_removes_only_one() {
        // setup channels
        let (out_tx, mut out_rx) = mpsc::channel::<StompItem>(8);
        let (_in_tx, in_rx) = mpsc::channel::<Frame>(8);
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let subscriptions: Arc<Mutex<Subscriptions>> = Arc::new(Mutex::new(HashMap::new()));
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));

        let sub_id_counter = Arc::new(AtomicU64::new(1));

        // create a subscription entry s2 with client-individual ack
        let (sub_sender, _sub_rx) = mpsc::channel::<Frame>(4);
        {
            let mut map = subscriptions.lock().await;
            map.insert(
                "/queue/y".to_string(),
                vec![SubscriptionEntry {
                    id: "s2".to_string(),
                    sender: sub_sender,
                    ack: "client-individual".to_string(),
                    headers: Vec::new(),
                }],
            );
        }

        // fill pending queue for s2: a,b,c
        {
            let mut p = pending.lock().await;
            let mut q = VecDeque::new();
            q.push_back((
                "a".to_string(),
                make_message("a", Some("s2"), Some("/queue/y")),
            ));
            q.push_back((
                "b".to_string(),
                make_message("b", Some("s2"), Some("/queue/y")),
            ));
            q.push_back((
                "c".to_string(),
                make_message("c", Some("s2"), Some("/queue/y")),
            ));
            p.insert("s2".to_string(), q);
        }

        let conn = Connection {
            outbound_tx: out_tx,
            inbound_rx: Arc::new(Mutex::new(in_rx)),
            shutdown_tx,
            subscriptions: subscriptions.clone(),
            sub_id_counter,
            pending: pending.clone(),
            pending_receipts: Arc::new(Mutex::new(HashMap::new())),
        };

        // ack only 'b' individually
        conn.ack("s2", "b").await.expect("ack failed");

        // verify pending for s2 contains a and c
        {
            let p = pending.lock().await;
            let q = p.get("s2").expect("missing s2");
            assert_eq!(q.len(), 2);
            assert_eq!(q[0].0, "a");
            assert_eq!(q[1].0, "c");
        }

        // verify an ACK frame was emitted
        if let Some(item) = out_rx.recv().await {
            match item {
                StompItem::Frame(f) => assert_eq!(f.command, "ACK"),
                _ => panic!("expected frame"),
            }
        } else {
            panic!("no outbound frame sent")
        }
    }

    #[tokio::test]
    async fn test_subscription_receive_delivers_message() {
        // setup channels
        let (out_tx, _out_rx) = mpsc::channel::<StompItem>(8);
        let (_in_tx, in_rx) = mpsc::channel::<Frame>(8);
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let subscriptions: Arc<Mutex<Subscriptions>> = Arc::new(Mutex::new(HashMap::new()));
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));

        let sub_id_counter = Arc::new(AtomicU64::new(1));

        let conn = Connection {
            outbound_tx: out_tx,
            inbound_rx: Arc::new(Mutex::new(in_rx)),
            shutdown_tx,
            subscriptions: subscriptions.clone(),
            sub_id_counter,
            pending: pending.clone(),
            pending_receipts: Arc::new(Mutex::new(HashMap::new())),
        };

        // subscribe
        let subscription = conn
            .subscribe("/queue/test", AckMode::Auto)
            .await
            .expect("subscribe failed");

        // find the sender stored in the subscriptions map and push a message
        {
            let map = conn.subscriptions.lock().await;
            let vec = map.get("/queue/test").expect("missing subscription vec");
            let sender = &vec[0].sender;
            let f = make_message("m1", Some(&vec[0].id), Some("/queue/test"));
            sender.try_send(f).expect("send to subscription failed");
        }

        // consume from the subscription receiver
        let mut rx = subscription.into_receiver();
        if let Some(received) = rx.recv().await {
            assert_eq!(received.command, "MESSAGE");
            // message-id header should be present
            let mut found = false;
            for (k, _v) in &received.headers {
                if k.to_lowercase() == "message-id" {
                    found = true;
                    break;
                }
            }
            assert!(found, "message-id header missing");
        } else {
            panic!("no message received on subscription")
        }
    }

    #[tokio::test]
    async fn test_subscription_ack_removes_pending_and_sends_ack() {
        // setup channels
        let (out_tx, mut out_rx) = mpsc::channel::<StompItem>(8);
        let (_in_tx, in_rx) = mpsc::channel::<Frame>(8);
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let subscriptions: Arc<Mutex<Subscriptions>> = Arc::new(Mutex::new(HashMap::new()));
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));

        let sub_id_counter = Arc::new(AtomicU64::new(1));

        let conn = Connection {
            outbound_tx: out_tx,
            inbound_rx: Arc::new(Mutex::new(in_rx)),
            shutdown_tx,
            subscriptions: subscriptions.clone(),
            sub_id_counter,
            pending: pending.clone(),
            pending_receipts: Arc::new(Mutex::new(HashMap::new())),
        };

        // subscribe with client ack
        let subscription = conn
            .subscribe("/queue/ack", AckMode::Client)
            .await
            .expect("subscribe failed");

        let sub_id = subscription.id().to_string();

        // drain any initial outbound frames (SUBSCRIBE) emitted by subscribe()
        while out_rx.try_recv().is_ok() {}

        // populate pending queue for this subscription
        {
            let mut p = conn.pending.lock().await;
            let mut q = VecDeque::new();
            q.push_back((
                "mid-1".to_string(),
                make_message("mid-1", Some(&sub_id), Some("/queue/ack")),
            ));
            p.insert(sub_id.clone(), q);
        }

        // ack the message via the subscription helper
        subscription.ack("mid-1").await.expect("ack failed");

        // ensure pending queue no longer contains the message
        {
            let p = conn.pending.lock().await;
            assert!(p.get(&sub_id).is_none() || p.get(&sub_id).unwrap().is_empty());
        }

        // verify an ACK frame was emitted
        if let Some(item) = out_rx.recv().await {
            match item {
                StompItem::Frame(f) => assert_eq!(f.command, "ACK"),
                _ => panic!("expected frame"),
            }
        } else {
            panic!("no outbound frame sent")
        }
    }

    // Helper function to create a test connection and output receiver
    fn setup_test_connection() -> (Connection, mpsc::Receiver<StompItem>) {
        let (out_tx, out_rx) = mpsc::channel::<StompItem>(8);
        let (_in_tx, in_rx) = mpsc::channel::<Frame>(8);
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let subscriptions: Arc<Mutex<Subscriptions>> = Arc::new(Mutex::new(HashMap::new()));
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let sub_id_counter = Arc::new(AtomicU64::new(1));

        let conn = Connection {
            outbound_tx: out_tx,
            inbound_rx: Arc::new(Mutex::new(in_rx)),
            shutdown_tx,
            subscriptions,
            sub_id_counter,
            pending,
            pending_receipts: Arc::new(Mutex::new(HashMap::new())),
        };

        (conn, out_rx)
    }

    // Helper function to verify a frame with a transaction header
    fn verify_transaction_frame(frame: Frame, expected_command: &str, expected_tx_id: &str) {
        assert_eq!(frame.command, expected_command);
        assert!(
            frame
                .headers
                .iter()
                .any(|(k, v)| k == "transaction" && v == expected_tx_id),
            "transaction header with id '{}' not found",
            expected_tx_id
        );
    }

    #[tokio::test]
    async fn test_begin_transaction_sends_frame() {
        let (conn, mut out_rx) = setup_test_connection();

        conn.begin("tx1").await.expect("begin failed");

        // verify BEGIN frame was emitted
        if let Some(StompItem::Frame(f)) = out_rx.recv().await {
            verify_transaction_frame(f, "BEGIN", "tx1");
        } else {
            panic!("no outbound frame sent")
        }
    }

    #[tokio::test]
    async fn test_commit_transaction_sends_frame() {
        let (conn, mut out_rx) = setup_test_connection();

        conn.commit("tx1").await.expect("commit failed");

        // verify COMMIT frame was emitted
        if let Some(StompItem::Frame(f)) = out_rx.recv().await {
            verify_transaction_frame(f, "COMMIT", "tx1");
        } else {
            panic!("no outbound frame sent")
        }
    }

    #[tokio::test]
    async fn test_abort_transaction_sends_frame() {
        let (conn, mut out_rx) = setup_test_connection();

        conn.abort("tx1").await.expect("abort failed");

        // verify ABORT frame was emitted
        if let Some(StompItem::Frame(f)) = out_rx.recv().await {
            verify_transaction_frame(f, "ABORT", "tx1");
        } else {
            panic!("no outbound frame sent")
        }
    }
}
