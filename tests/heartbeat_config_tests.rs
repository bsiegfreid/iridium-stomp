//! Tests for Heartbeat configuration and Connection constants (Issue #36)
//!
//! These tests verify:
//! - Heartbeat struct creation and methods
//! - Connection::NO_HEARTBEAT and Connection::DEFAULT_HEARTBEAT constants
//! - Display trait implementation
//! - Default trait implementation

use iridium_stomp::{Connection, Heartbeat};
use std::time::Duration;

// ============================================================================
// Heartbeat struct tests
// ============================================================================

#[test]
fn heartbeat_new() {
    let hb = Heartbeat::new(5000, 10000);
    assert_eq!(hb.send_ms, 5000);
    assert_eq!(hb.receive_ms, 10000);
}

#[test]
fn heartbeat_disabled() {
    let hb = Heartbeat::disabled();
    assert_eq!(hb.send_ms, 0);
    assert_eq!(hb.receive_ms, 0);
}

#[test]
fn heartbeat_default() {
    let hb = Heartbeat::default();
    assert_eq!(hb.send_ms, 10000);
    assert_eq!(hb.receive_ms, 10000);
}

#[test]
fn heartbeat_from_duration() {
    let hb = Heartbeat::from_duration(Duration::from_secs(15));
    assert_eq!(hb.send_ms, 15000);
    assert_eq!(hb.receive_ms, 15000);
}

#[test]
fn heartbeat_from_duration_millis() {
    let hb = Heartbeat::from_duration(Duration::from_millis(7500));
    assert_eq!(hb.send_ms, 7500);
    assert_eq!(hb.receive_ms, 7500);
}

#[test]
fn heartbeat_display() {
    let hb = Heartbeat::new(5000, 10000);
    assert_eq!(hb.to_string(), "5000,10000");
}

#[test]
fn heartbeat_display_disabled() {
    let hb = Heartbeat::disabled();
    assert_eq!(hb.to_string(), "0,0");
}

#[test]
fn heartbeat_display_default() {
    let hb = Heartbeat::default();
    assert_eq!(hb.to_string(), "10000,10000");
}

#[test]
#[allow(clippy::clone_on_copy)]
fn heartbeat_clone() {
    // Test that Clone works even though Copy is also implemented
    let hb1 = Heartbeat::new(5000, 10000);
    let hb2 = hb1.clone();
    assert_eq!(hb1, hb2);
}

#[test]
fn heartbeat_copy() {
    let hb1 = Heartbeat::new(5000, 10000);
    let hb2 = hb1; // Copy
    assert_eq!(hb1, hb2);
}

#[test]
fn heartbeat_eq() {
    let hb1 = Heartbeat::new(5000, 10000);
    let hb2 = Heartbeat::new(5000, 10000);
    assert_eq!(hb1, hb2);
}

#[test]
fn heartbeat_ne() {
    let hb1 = Heartbeat::new(5000, 10000);
    let hb2 = Heartbeat::new(5000, 15000);
    assert_ne!(hb1, hb2);
}

#[test]
fn heartbeat_debug() {
    let hb = Heartbeat::new(5000, 10000);
    let debug = format!("{:?}", hb);
    assert!(debug.contains("Heartbeat"));
    assert!(debug.contains("5000"));
    assert!(debug.contains("10000"));
}

// ============================================================================
// Connection constants tests
// ============================================================================

#[test]
fn connection_no_heartbeat_constant() {
    assert_eq!(Connection::NO_HEARTBEAT, "0,0");
}

#[test]
fn connection_default_heartbeat_constant() {
    assert_eq!(Connection::DEFAULT_HEARTBEAT, "10000,10000");
}

#[test]
fn heartbeat_disabled_matches_constant() {
    assert_eq!(Heartbeat::disabled().to_string(), Connection::NO_HEARTBEAT);
}

#[test]
fn heartbeat_default_matches_constant() {
    assert_eq!(
        Heartbeat::default().to_string(),
        Connection::DEFAULT_HEARTBEAT
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn heartbeat_zero_send_nonzero_receive() {
    let hb = Heartbeat::new(0, 10000);
    assert_eq!(hb.to_string(), "0,10000");
}

#[test]
fn heartbeat_nonzero_send_zero_receive() {
    let hb = Heartbeat::new(10000, 0);
    assert_eq!(hb.to_string(), "10000,0");
}

#[test]
fn heartbeat_large_values() {
    let hb = Heartbeat::new(u32::MAX, u32::MAX);
    assert_eq!(hb.send_ms, u32::MAX);
    assert_eq!(hb.receive_ms, u32::MAX);
    let display = hb.to_string();
    assert!(display.contains(&u32::MAX.to_string()));
}

#[test]
fn heartbeat_one_millisecond() {
    let hb = Heartbeat::new(1, 1);
    assert_eq!(hb.to_string(), "1,1");
}
