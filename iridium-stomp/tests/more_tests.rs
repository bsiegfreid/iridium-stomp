use bytes::BytesMut;
use iridium_stomp::codec::{StompCodec, StompItem};
use iridium_stomp::frame::Frame;
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn partial_frame_decoding() {
    let mut codec = StompCodec::new();
    let raw = b"SEND\ncontent-length:11\n\nhello world\0";

    let mut buf = BytesMut::new();
    // feed first 10 bytes -> not enough
    buf.extend_from_slice(&raw[..10]);
    let r1 = codec.decode(&mut buf).expect("decode failed");
    assert!(r1.is_none(), "expected None when partial data provided");

    // feed the rest
    buf.extend_from_slice(&raw[10..]);
    let r2 = codec.decode(&mut buf).expect("decode failed").expect("no item");
    match r2 {
        StompItem::Frame(f) => assert_eq!(f.body, b"hello world".to_vec()),
        _ => panic!("expected frame"),
    }
}

#[test]
fn large_content_length_roundtrip() {
    let mut codec = StompCodec::new();
    // large body (10k 'x')
    let body = vec![b'x'; 10_000];
    let mut frame = Frame::new("SEND");
    frame = frame.header("content-length", &body.len().to_string()).set_body(body.clone());

    let mut dst = BytesMut::new();
    codec.encode(StompItem::Frame(frame), &mut dst).expect("encode failed");

    let mut dec = StompCodec::new();
    let item = dec.decode(&mut dst).expect("decode failed").expect("no item");
    match item {
        StompItem::Frame(f) => assert_eq!(f.body, body),
        _ => panic!("expected frame"),
    }
}

#[test]
fn malformed_header_invalid_utf8_fails() {
    let mut codec = StompCodec::new();
    // header key contains invalid utf8 (0xFF)
    let raw = b"SEND\nk\xFFey:value\n\n\0";
    let mut buf = BytesMut::from(&raw[..]);
    let res = codec.decode(&mut buf);
    assert!(res.is_err(), "expected error for invalid utf8 in header");
}

#[test]
fn encode_no_content_length_for_utf8_body() {
    let mut codec = StompCodec::new();
    let frame = Frame::new("SEND").set_body(b"hello".to_vec());
    let mut dst = BytesMut::new();
    codec.encode(StompItem::Frame(frame), &mut dst).expect("encode failed");
    let s = String::from_utf8_lossy(&dst);
    assert!(!s.contains("content-length:"), "unexpected content-length header for utf8 body: {}", s);
}
