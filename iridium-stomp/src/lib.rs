//! Top-level library exports for the `iridium-stomp` crate.
//!
//! This crate provides a small STOMP 1.2 client codec and connection manager
//! (heartbeat negotiation, basic reconnect behavior). The primary types are
//! exported here for easy consumption.
//!
//! Public API
//! - `StompCodec` / `StompItem`: the tokio util codec and item type used to
//!   encode/decode STOMP frames and heartbeats.
//! - `Connection`: high-level connection manager that handles connect,
//!   heartbeat negotiation, background IO, and simple reconnect logic.
//! - `Frame`: a small POJO-like representation of a STOMP frame.

pub mod codec;
pub mod connection;
pub mod frame;
pub mod parser;
pub mod subscription;

/// Re-export the codec types (`StompCodec`, `StompItem`) for easy use with
/// `tokio_util::codec::Framed` and tests.
pub use codec::{StompCodec, StompItem};

/// Re-export the high-level `Connection` and the heartbeat helper functions.
pub use connection::{Connection, negotiate_heartbeats, parse_heartbeat_header};

/// Re-export the `Frame` type used to construct/send and receive frames.
pub use frame::Frame;
pub use subscription::Subscription;
pub use subscription::SubscriptionOptions;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_frame_display() {
        let f = Frame::new("CONNECT")
            .header("accept-version", "1.2")
            .set_body(b"hello".to_vec());
        let s = format!("{}", f);
        assert!(s.contains("CONNECT"));
        assert!(s.contains("Body (5 bytes)"));
    }
}
