//! Tests for transaction frame encoding and decoding

use bytes::BytesMut;
use iridium_stomp::{Frame, StompCodec, StompItem};
use tokio_util::codec::{Decoder, Encoder};

/// Helper function to verify a decoded frame has the expected command and transaction header
fn verify_transaction_frame(frame: Frame, expected_command: &str, expected_tx_id: &str) {
    assert_eq!(frame.command, expected_command);
    let tx_header = frame
        .headers
        .iter()
        .find(|(k, _)| k == "transaction")
        .map(|(_, v)| v.as_str());
    assert_eq!(tx_header, Some(expected_tx_id));
}

/// Helper function to encode and decode a transaction frame
fn encode_decode_transaction_frame(
    command: &str,
    tx_id: &str,
) -> Result<Frame, Box<dyn std::error::Error>> {
    let mut codec = StompCodec::new();
    let mut buf = BytesMut::new();

    // Encode frame
    let frame = Frame::new(command).header("transaction", tx_id);
    codec.encode(StompItem::Frame(frame), &mut buf)?;

    // Decode frame
    match codec.decode(&mut buf)? {
        Some(StompItem::Frame(f)) => Ok(f),
        _ => Err("Expected Frame, got something else".into()),
    }
}

#[test]
fn test_begin_frame_encode_decode() {
    let frame = encode_decode_transaction_frame("BEGIN", "tx1").expect("encode/decode failed");
    verify_transaction_frame(frame, "BEGIN", "tx1");
}

#[test]
fn test_commit_frame_encode_decode() {
    let frame = encode_decode_transaction_frame("COMMIT", "tx2").expect("encode/decode failed");
    verify_transaction_frame(frame, "COMMIT", "tx2");
}

#[test]
fn test_abort_frame_encode_decode() {
    let frame = encode_decode_transaction_frame("ABORT", "tx3").expect("encode/decode failed");
    verify_transaction_frame(frame, "ABORT", "tx3");
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
