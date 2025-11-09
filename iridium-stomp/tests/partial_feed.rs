use bytes::BytesMut;
use iridium_stomp::codec::{StompCodec, StompItem};
use tokio_util::codec::Decoder;

// Feed bytes one at a time to the decoder and assert it only returns a
// frame once the entire frame (including trailing NUL when required) is
// present. This ensures the decoder is resilient to incremental arrival.
#[test]
fn byte_by_byte_content_length() {
    let mut codec = StompCodec::new();
    let raw = b"SEND\ncontent-length:5\n\nhello\0";

    let mut buf = BytesMut::new();
    for i in 0..raw.len() {
        buf.extend_from_slice(&raw[i..i+1]);
        let res = codec.decode(&mut buf).expect("decode failed");
        if i < raw.len() - 1 {
            // until the last byte (NUL) we should not have a full item
            assert!(res.is_none(), "decoder produced item too early at byte {}", i);
        } else {
            // final byte should produce a frame
            let item = res.expect("expected item after final byte");
            match item {
                StompItem::Frame(f) => assert_eq!(f.body, b"hello".to_vec()),
                _ => panic!("expected frame"),
            }
        }
    }
}

#[test]
fn small_chunk_null_terminated() {
    let mut codec = StompCodec::new();
    // null-terminated frame; simulate arrival in small chunks
    let raw = b"SEND\n\nchunked body\0";
    let mut buf = BytesMut::new();

    // feed in chunks of size 3
    let mut offset = 0usize;
    while offset < raw.len() {
        let end = (offset + 3).min(raw.len());
        buf.extend_from_slice(&raw[offset..end]);
        let res = codec.decode(&mut buf).expect("decode failed");
        if end < raw.len() {
            assert!(res.is_none(), "decoder produced item too early at offset {}", end);
        } else {
            let item = res.expect("expected item after final chunk");
            match item {
                StompItem::Frame(f) => assert_eq!(f.body, b"chunked body".to_vec()),
                _ => panic!("expected frame"),
            }
        }
        offset = end;
    }
}
