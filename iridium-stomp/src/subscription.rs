use crate::connection::ConnError;
use crate::frame::Frame;
use crate::connection::Connection;
use tokio::sync::mpsc;

/// A lightweight handle returned from `Connection::subscribe` that packages the
/// subscription id, destination, and the receiving side of the subscription.
///
/// The `Subscription` provides convenience helpers for acknowledging or
/// negative-acknowledging messages; these delegate to the underlying
/// `Connection` handle.
pub struct Subscription {
    id: String,
    destination: String,
    receiver: mpsc::Receiver<Frame>,
    conn: Connection,
}

impl Subscription {
    pub(crate) fn new(id: String, destination: String, receiver: mpsc::Receiver<Frame>, conn: Connection) -> Self {
        Self { id, destination, receiver, conn }
    }

    /// Returns the local subscription id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the destination this subscription listens to.
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Consume the `Subscription` and return the underlying receiver so the
    /// caller can drive message handling directly.
    pub fn into_receiver(self) -> mpsc::Receiver<Frame> {
        self.receiver
    }

    /// Acknowledge a message by its `message-id` header. Delegates to
    /// `Connection::ack` using the local subscription id.
    pub async fn ack(&self, message_id: &str) -> Result<(), ConnError> {
        self.conn.ack(&self.id, message_id).await
    }

    /// Negative-acknowledge a message by its `message-id` header.
    pub async fn nack(&self, message_id: &str) -> Result<(), ConnError> {
        self.conn.nack(&self.id, message_id).await
    }
}
