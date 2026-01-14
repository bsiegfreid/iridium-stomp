//! Unit tests for heartbeat encoding and decoding in the STOMP codec.

use bytes::BytesMut;
use iridium_stomp::Frame;
use iridium_stomp::codec::{StompCodec, StompItem};
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn decode_single_lf_as_heartbeat() {
    let mut codec = StompCodec::new();
    let mut buf = BytesMut::from(&[0x0Au8][..]);
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item, StompItem::Heartbeat);
    assert!(
        buf.is_empty(),
        "buffer should be empty after consuming heartbeat"
    );
}

#[test]
fn decode_multiple_consecutive_heartbeats() {
    let mut codec = StompCodec::new();
    let mut buf = BytesMut::from(&[0x0A, 0x0A, 0x0A][..]);

    // First heartbeat
    let item1 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item1, StompItem::Heartbeat);
    assert_eq!(buf.len(), 2);

    // Second heartbeat
    let item2 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item2, StompItem::Heartbeat);
    assert_eq!(buf.len(), 1);

    // Third heartbeat
    let item3 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item3, StompItem::Heartbeat);
    assert!(buf.is_empty());
}

#[test]
fn decode_heartbeat_before_frame() {
    let mut codec = StompCodec::new();
    // Heartbeat (LF) followed by a SEND frame
    let data = b"\nSEND\ndestination:/queue/test\n\nhello\0";
    let mut buf = BytesMut::from(&data[..]);

    // First decode returns heartbeat
    let item1 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item1, StompItem::Heartbeat);

    // Second decode returns frame
    let item2 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    match item2 {
        StompItem::Frame(f) => {
            assert_eq!(f.command, "SEND");
            assert_eq!(f.body, b"hello");
        }
        _ => panic!("expected frame, got {:?}", item2),
    }
}

#[test]
fn decode_heartbeat_after_frame() {
    let mut codec = StompCodec::new();
    // Frame followed by TWO LFs - first is consumed as optional trailing LF,
    // second is a separate heartbeat per STOMP spec
    let data = b"SEND\ndestination:/queue/test\n\nhello\0\n\n";
    let mut buf = BytesMut::from(&data[..]);

    // First decode returns frame (consumes optional trailing LF)
    let item1 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    match item1 {
        StompItem::Frame(f) => {
            assert_eq!(f.command, "SEND");
        }
        _ => panic!("expected frame, got {:?}", item1),
    }

    // Second decode returns heartbeat
    let item2 = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item2, StompItem::Heartbeat);
}

#[test]
fn encode_heartbeat() {
    let mut codec = StompCodec::new();
    let mut dst = BytesMut::new();
    codec
        .encode(StompItem::Heartbeat, &mut dst)
        .expect("encode failed");
    assert_eq!(&dst[..], &[0x0Au8]);
}

#[test]
fn roundtrip_heartbeat() {
    let mut codec = StompCodec::new();

    // Encode
    let mut encoded = BytesMut::new();
    codec
        .encode(StompItem::Heartbeat, &mut encoded)
        .expect("encode failed");

    // Decode
    let decoded = codec
        .decode(&mut encoded)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(decoded, StompItem::Heartbeat);
    assert!(encoded.is_empty());
}

#[test]
fn interleaved_heartbeats_and_frames() {
    let mut codec = StompCodec::new();
    // HB, Frame (with trailing LF consumed), HB, Frame (with trailing LF consumed), HB
    // Note: The LF after NUL is consumed as optional trailing per STOMP spec
    // So we need extra LFs for actual heartbeats
    let data = b"\nSEND\n\n\0\n\nMESSAGE\nmessage-id:1\n\nbody\0\n\n";
    let mut buf = BytesMut::from(&data[..]);

    // 1. Heartbeat
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item, StompItem::Heartbeat);

    // 2. SEND frame (consumes trailing LF)
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    match &item {
        StompItem::Frame(f) => assert_eq!(f.command, "SEND"),
        _ => panic!("expected SEND frame"),
    }

    // 3. Heartbeat
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item, StompItem::Heartbeat);

    // 4. MESSAGE frame (consumes trailing LF)
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    match &item {
        StompItem::Frame(f) => {
            assert_eq!(f.command, "MESSAGE");
            assert_eq!(f.body, b"body");
        }
        _ => panic!("expected MESSAGE frame"),
    }

    // 5. Heartbeat
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item, StompItem::Heartbeat);

    assert!(buf.is_empty());
}

#[test]
fn heartbeat_does_not_corrupt_subsequent_frame_data() {
    let mut codec = StompCodec::new();
    // This tests that consuming a heartbeat correctly advances the buffer
    // and doesn't leave any partial state
    let data = b"\nCONNECT\naccept-version:1.2\nhost:/\n\n\0";
    let mut buf = BytesMut::from(&data[..]);

    // Heartbeat
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    assert_eq!(item, StompItem::Heartbeat);

    // Frame should decode correctly with all headers intact
    let item = codec
        .decode(&mut buf)
        .expect("decode failed")
        .expect("no item");
    match item {
        StompItem::Frame(f) => {
            assert_eq!(f.command, "CONNECT");
            assert_eq!(f.headers.len(), 2);
            assert!(
                f.headers
                    .iter()
                    .any(|(k, v)| k == "accept-version" && v == "1.2")
            );
            assert!(f.headers.iter().any(|(k, v)| k == "host" && v == "/"));
        }
        _ => panic!("expected CONNECT frame"),
    }
}

#[test]
fn encode_heartbeat_multiple_times() {
    let mut codec = StompCodec::new();
    let mut dst = BytesMut::new();

    codec
        .encode(StompItem::Heartbeat, &mut dst)
        .expect("encode failed");
    codec
        .encode(StompItem::Heartbeat, &mut dst)
        .expect("encode failed");
    codec
        .encode(StompItem::Heartbeat, &mut dst)
        .expect("encode failed");

    assert_eq!(&dst[..], &[0x0A, 0x0A, 0x0A]);
}

#[test]
fn encode_frame_then_heartbeat() {
    let mut codec = StompCodec::new();
    let mut dst = BytesMut::new();

    let frame = Frame::new("SEND")
        .header("destination", "/queue/test")
        .set_body(b"hello".to_vec());

    codec
        .encode(StompItem::Frame(frame), &mut dst)
        .expect("encode failed");
    codec
        .encode(StompItem::Heartbeat, &mut dst)
        .expect("encode failed");

    // Verify it ends with NUL then LF
    let len = dst.len();
    assert_eq!(dst[len - 2], 0x00); // NUL terminator
    assert_eq!(dst[len - 1], 0x0A); // Heartbeat LF
}
