use bytes::BytesMut;
use iridium_stomp::codec::{StompCodec, StompItem};
use iridium_stomp::frame::Frame;
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn decode_content_length_frame() {
    let mut codec = StompCodec::new();
    // SEND frame with content-length:5 and trailing NUL
    let raw = b"SEND\ncontent-length:5\n\nhello\0";
    let mut buf = BytesMut::from(&raw[..]);
    let item = codec.decode(&mut buf).expect("decode failed").expect("no item");
    match item {
        StompItem::Frame(f) => {
            assert_eq!(f.command, "SEND");
            assert_eq!(f.body, b"hello".to_vec());
        }
        _ => panic!("expected frame"),
    }
}

#[test]
fn decode_null_terminated_frame() {
    let mut codec = StompCodec::new();
    let raw = b"SEND\n\nhi\0";
    let mut buf = BytesMut::from(&raw[..]);
    let item = codec.decode(&mut buf).expect("decode failed").expect("no item");
    match item {
        StompItem::Frame(f) => {
            assert_eq!(f.command, "SEND");
            assert_eq!(f.body, b"hi".to_vec());
        }
        _ => panic!("expected frame"),
    }
}

#[test]
fn decode_missing_nul_after_content_length_errors() {
    let mut codec = StompCodec::new();
    let raw = b"SEND\ncontent-length:5\n\nhello"; // no trailing NUL
    let mut buf = BytesMut::from(&raw[..]);
    let res = codec.decode(&mut buf).expect("decode call failed");
    // When a content-length body is present but the trailing NUL has not yet
    // arrived, the decoder should return Ok(None) to indicate it needs more
    // bytes rather than error immediately.
    assert!(res.is_none(), "expected None (need more bytes) for missing NUL");
}

#[test]
fn encode_includes_content_length_for_binary_body() {
    let mut codec = StompCodec::new();
    let frame = Frame::new("SEND").set_body(vec![0u8, 1, 2, 3]);
    let mut dst = BytesMut::new();
    codec.encode(StompItem::Frame(frame), &mut dst).expect("encode failed");
    let s = String::from_utf8_lossy(&dst);
    // Should include content-length header
    assert!(s.contains("content-length:"), "encoded frame missing content-length header: {}", s);
}
