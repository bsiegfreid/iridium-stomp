pub mod codec;
pub mod connection;
pub mod frame;

pub use codec::{StompCodec, StompItem};
pub use connection::{Connection, negotiate_heartbeats, parse_heartbeat_header};
pub use frame::Frame;

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
