//! Tests for RECEIPT frame support (Issue #33)
//!
//! These tests verify:
//! - Frame::receipt() builder method
//! - Receipt ID generation
//! - RECEIPT frame parsing
//! - Timeout handling for receipts

use iridium_stomp::Frame;

// ============================================================================
// Frame::receipt() builder tests
// ============================================================================

#[test]
fn frame_receipt_adds_header() {
    let frame = Frame::new("SEND")
        .header("destination", "/queue/test")
        .receipt("receipt-123");

    assert_eq!(frame.get_header("receipt"), Some("receipt-123"));
}

#[test]
fn frame_receipt_preserves_other_headers() {
    let frame = Frame::new("SEND")
        .header("destination", "/queue/test")
        .header("content-type", "text/plain")
        .receipt("rcpt-456")
        .set_body(b"hello".to_vec());

    assert_eq!(frame.command, "SEND");
    assert_eq!(frame.get_header("destination"), Some("/queue/test"));
    assert_eq!(frame.get_header("content-type"), Some("text/plain"));
    assert_eq!(frame.get_header("receipt"), Some("rcpt-456"));
    assert_eq!(frame.body, b"hello");
}

#[test]
fn frame_receipt_empty_id() {
    let frame = Frame::new("SEND").receipt("");
    assert_eq!(frame.get_header("receipt"), Some(""));
}

#[test]
fn frame_receipt_special_characters() {
    // Receipt IDs with special characters should work
    let frame = Frame::new("SEND").receipt("rcpt-uuid-12345-abcde");
    assert_eq!(frame.get_header("receipt"), Some("rcpt-uuid-12345-abcde"));
}

#[test]
fn frame_receipt_can_be_chained() {
    // Verify fluent API works with receipt in the chain
    let frame = Frame::new("SEND")
        .header("destination", "/queue/test")
        .receipt("r1")
        .header("custom", "value")
        .set_body(b"data".to_vec());

    assert_eq!(frame.get_header("receipt"), Some("r1"));
    assert_eq!(frame.get_header("custom"), Some("value"));
}

// ============================================================================
// Codec tests for RECEIPT frame parsing
// ============================================================================

mod codec_receipt_tests {
    use bytes::BytesMut;
    use tokio_util::codec::Decoder;

    // We need to import the codec - this tests internal behavior
    // Note: StompCodec is pub so we can test it directly

    #[test]
    fn decode_receipt_frame() {
        use iridium_stomp::codec::{StompCodec, StompItem};

        let mut codec = StompCodec::new();
        let mut buf = BytesMut::from("RECEIPT\nreceipt-id:msg-12345\n\n\0");

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());

        if let Some(StompItem::Frame(frame)) = result {
            assert_eq!(frame.command, "RECEIPT");
            assert_eq!(frame.get_header("receipt-id"), Some("msg-12345"));
            assert!(frame.body.is_empty());
        } else {
            panic!("Expected Frame, got Heartbeat");
        }
    }

    #[test]
    fn decode_receipt_frame_with_escaped_id() {
        use iridium_stomp::codec::{StompCodec, StompItem};

        let mut codec = StompCodec::new();
        // Receipt ID with escaped colon: "foo:bar" becomes "foo\cbar" on wire
        let mut buf = BytesMut::from("RECEIPT\nreceipt-id:foo\\cbar\n\n\0");

        let result = codec.decode(&mut buf).unwrap();
        assert!(result.is_some());

        if let Some(StompItem::Frame(frame)) = result {
            assert_eq!(frame.command, "RECEIPT");
            // After unescaping, the colon should be restored
            assert_eq!(frame.get_header("receipt-id"), Some("foo:bar"));
        } else {
            panic!("Expected Frame, got Heartbeat");
        }
    }

    #[test]
    fn encode_frame_with_receipt_header() {
        use iridium_stomp::Frame;
        use iridium_stomp::codec::{StompCodec, StompItem};
        use tokio_util::codec::Encoder;

        let mut codec = StompCodec::new();
        let frame = Frame::new("SEND")
            .header("destination", "/queue/test")
            .receipt("rcpt-001")
            .set_body(b"hello".to_vec());

        let mut buf = BytesMut::new();
        codec.encode(StompItem::Frame(frame), &mut buf).unwrap();

        let encoded = String::from_utf8_lossy(&buf);
        assert!(encoded.contains("SEND\n"));
        assert!(encoded.contains("destination:/queue/test\n"));
        assert!(encoded.contains("receipt:rcpt-001\n"));
        assert!(encoded.contains("hello"));
    }
}

// ============================================================================
// ConnError::ReceiptTimeout tests
// ============================================================================

mod error_tests {
    use iridium_stomp::ConnError;

    #[test]
    fn receipt_timeout_error_display() {
        let err = ConnError::ReceiptTimeout("rcpt-123".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("rcpt-123"));
        assert!(msg.contains("timeout"));
    }

    #[test]
    fn receipt_timeout_error_debug() {
        let err = ConnError::ReceiptTimeout("test-id".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("ReceiptTimeout"));
        assert!(debug.contains("test-id"));
    }
}

// ============================================================================
// Unit tests for receipt ID format
// ============================================================================

#[test]
fn receipt_id_format_is_valid_stomp() {
    // STOMP receipt IDs should not contain characters that need escaping
    // in header values (newline, colon in value is ok but we avoid it)
    let test_ids = ["rcpt-1", "rcpt-12345", "rcpt-999999999"];

    for id in test_ids {
        let frame = Frame::new("SEND").receipt(id);
        let receipt = frame.get_header("receipt").unwrap();
        // Verify no problematic characters
        assert!(!receipt.contains('\n'));
        assert!(!receipt.contains('\r'));
        assert!(!receipt.contains('\0'));
    }
}
