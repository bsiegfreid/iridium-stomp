//! Unit tests for connection-related types.
//!
//! Note: Full Connection testing requires network I/O and is covered by
//! inline tests in connection.rs and the integration smoke test. This file
//! tests the public types and error handling.

use iridium_stomp::connection::{AckMode, ConnError};
use std::io;

// =============================================================================
// AckMode Tests
// =============================================================================

#[test]
fn ack_mode_debug() {
    assert!(format!("{:?}", AckMode::Auto).contains("Auto"));
    assert!(format!("{:?}", AckMode::Client).contains("Client"));
    assert!(format!("{:?}", AckMode::ClientIndividual).contains("ClientIndividual"));
}

#[test]
fn ack_mode_copy() {
    let mode = AckMode::Client;
    let copied = mode; // AckMode is Copy
    assert_eq!(mode, copied);
}

#[test]
fn ack_mode_eq() {
    assert_eq!(AckMode::Auto, AckMode::Auto);
    assert_eq!(AckMode::Client, AckMode::Client);
    assert_eq!(AckMode::ClientIndividual, AckMode::ClientIndividual);
}

#[test]
fn ack_mode_ne() {
    assert_ne!(AckMode::Auto, AckMode::Client);
    assert_ne!(AckMode::Client, AckMode::ClientIndividual);
    assert_ne!(AckMode::Auto, AckMode::ClientIndividual);
}

// =============================================================================
// ConnError Tests
// =============================================================================

#[test]
fn conn_error_io_display() {
    let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
    let conn_err = ConnError::Io(io_err);
    let display = format!("{}", conn_err);
    assert!(display.contains("io error"));
    assert!(display.contains("connection refused"));
}

#[test]
fn conn_error_protocol_display() {
    let conn_err = ConnError::Protocol("invalid frame".to_string());
    let display = format!("{}", conn_err);
    assert!(display.contains("protocol error"));
    assert!(display.contains("invalid frame"));
}

#[test]
fn conn_error_io_from() {
    let io_err = io::Error::new(io::ErrorKind::TimedOut, "timeout");
    let conn_err: ConnError = io_err.into();
    match conn_err {
        ConnError::Io(e) => assert_eq!(e.kind(), io::ErrorKind::TimedOut),
        _ => panic!("expected Io variant"),
    }
}

#[test]
fn conn_error_debug() {
    let conn_err = ConnError::Protocol("test error".to_string());
    let debug = format!("{:?}", conn_err);
    assert!(debug.contains("Protocol"));
    assert!(debug.contains("test error"));
}

#[test]
fn conn_error_is_error_trait() {
    // Verify ConnError implements std::error::Error
    fn assert_error<E: std::error::Error>() {}
    assert_error::<ConnError>();
}
