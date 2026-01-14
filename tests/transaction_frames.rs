//! Tests for transaction frame encoding and decoding

use bytes::BytesMut;
use iridium_stomp::{Frame, StompCodec, StompItem};
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn test_begin_frame_encode_decode() {
    let mut codec = StompCodec::new();
    let mut buf = BytesMut::new();

    // Encode BEGIN frame
    let begin_frame = Frame::new("BEGIN").header("transaction", "tx1");
    codec
        .encode(StompItem::Frame(begin_frame), &mut buf)
        .expect("encode failed");

    // Decode BEGIN frame
    match codec.decode(&mut buf).expect("decode failed") {
        Some(StompItem::Frame(f)) => {
            assert_eq!(f.command, "BEGIN");
            // Verify transaction header
            let tx_header = f
                .headers
                .iter()
                .find(|(k, _)| k == "transaction")
                .map(|(_, v)| v.as_str());
            assert_eq!(tx_header, Some("tx1"));
        }
        _ => panic!("Expected Frame, got something else"),
    }
}

#[test]
fn test_commit_frame_encode_decode() {
    let mut codec = StompCodec::new();
    let mut buf = BytesMut::new();

    // Encode COMMIT frame
    let commit_frame = Frame::new("COMMIT").header("transaction", "tx2");
    codec
        .encode(StompItem::Frame(commit_frame), &mut buf)
        .expect("encode failed");

    // Decode COMMIT frame
    match codec.decode(&mut buf).expect("decode failed") {
        Some(StompItem::Frame(f)) => {
            assert_eq!(f.command, "COMMIT");
            // Verify transaction header
            let tx_header = f
                .headers
                .iter()
                .find(|(k, _)| k == "transaction")
                .map(|(_, v)| v.as_str());
            assert_eq!(tx_header, Some("tx2"));
        }
        _ => panic!("Expected Frame, got something else"),
    }
}

#[test]
fn test_abort_frame_encode_decode() {
    let mut codec = StompCodec::new();
    let mut buf = BytesMut::new();

    // Encode ABORT frame
    let abort_frame = Frame::new("ABORT").header("transaction", "tx3");
    codec
        .encode(StompItem::Frame(abort_frame), &mut buf)
        .expect("encode failed");

    // Decode ABORT frame
    match codec.decode(&mut buf).expect("decode failed") {
        Some(StompItem::Frame(f)) => {
            assert_eq!(f.command, "ABORT");
            // Verify transaction header
            let tx_header = f
                .headers
                .iter()
                .find(|(k, _)| k == "transaction")
                .map(|(_, v)| v.as_str());
            assert_eq!(tx_header, Some("tx3"));
        }
        _ => panic!("Expected Frame, got something else"),
    }
}

#[test]
fn test_transaction_frames_round_trip() {
    let mut codec = StompCodec::new();

    let frames = vec![
        Frame::new("BEGIN").header("transaction", "tx-test"),
        Frame::new("COMMIT").header("transaction", "tx-test"),
        Frame::new("ABORT").header("transaction", "tx-test"),
    ];

    for frame in frames {
        let mut buf = BytesMut::new();
        let cmd = frame.command.clone();

        // Encode
        codec
            .encode(StompItem::Frame(frame), &mut buf)
            .expect("encode failed");

        // Decode
        match codec.decode(&mut buf).expect("decode failed") {
            Some(StompItem::Frame(f)) => {
                assert_eq!(f.command, cmd);
            }
            _ => panic!("Expected Frame for command {}", cmd),
        }
    }
}
