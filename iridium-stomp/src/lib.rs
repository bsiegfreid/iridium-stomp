pub mod frame;
pub mod codec;

pub use frame::Frame;
pub use codec::{StompCodec, StompItem};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_frame_display() {
        let f = Frame::new("CONNECT").header("accept-version", "1.2").set_body(b"hello".to_vec());
        let s = format!("{}", f);
        assert!(s.contains("CONNECT"));
        assert!(s.contains("Body (5 bytes)"));
    }
}
