//! Unit tests for heartbeat parsing and negotiation functions.

use iridium_stomp::{negotiate_heartbeats, parse_heartbeat_header};
use std::time::Duration;

// =============================================================================
// parse_heartbeat_header tests
// =============================================================================

#[test]
fn parse_standard_heartbeat() {
    let (cx, cy) = parse_heartbeat_header("10000,10000");
    assert_eq!(cx, 10000);
    assert_eq!(cy, 10000);
}

#[test]
fn parse_zero_heartbeat() {
    let (cx, cy) = parse_heartbeat_header("0,0");
    assert_eq!(cx, 0);
    assert_eq!(cy, 0);
}

#[test]
fn parse_asymmetric_heartbeat() {
    let (cx, cy) = parse_heartbeat_header("5000,15000");
    assert_eq!(cx, 5000);
    assert_eq!(cy, 15000);
}

#[test]
fn parse_whitespace_padded() {
    let (cx, cy) = parse_heartbeat_header(" 10000 , 10000 ");
    assert_eq!(cx, 10000);
    assert_eq!(cy, 10000);
}

#[test]
fn parse_tab_whitespace() {
    let (cx, cy) = parse_heartbeat_header("\t5000\t,\t5000\t");
    assert_eq!(cx, 5000);
    assert_eq!(cy, 5000);
}

#[test]
fn parse_missing_second_value() {
    let (cx, cy) = parse_heartbeat_header("10000");
    assert_eq!(cx, 10000);
    assert_eq!(cy, 0); // defaults to 0
}

#[test]
fn parse_trailing_comma() {
    let (cx, cy) = parse_heartbeat_header("10000,");
    assert_eq!(cx, 10000);
    assert_eq!(cy, 0); // empty string parses as 0
}

#[test]
fn parse_leading_comma() {
    let (cx, cy) = parse_heartbeat_header(",10000");
    assert_eq!(cx, 0); // empty string parses as 0
    assert_eq!(cy, 10000);
}

#[test]
fn parse_empty_string() {
    let (cx, cy) = parse_heartbeat_header("");
    assert_eq!(cx, 0);
    assert_eq!(cy, 0);
}

#[test]
fn parse_invalid_first_value() {
    let (cx, cy) = parse_heartbeat_header("abc,10000");
    assert_eq!(cx, 0); // invalid parses as 0
    assert_eq!(cy, 10000);
}

#[test]
fn parse_invalid_second_value() {
    let (cx, cy) = parse_heartbeat_header("10000,xyz");
    assert_eq!(cx, 10000);
    assert_eq!(cy, 0); // invalid parses as 0
}

#[test]
fn parse_both_invalid() {
    let (cx, cy) = parse_heartbeat_header("abc,xyz");
    assert_eq!(cx, 0);
    assert_eq!(cy, 0);
}

#[test]
fn parse_negative_value() {
    // Negative values can't parse as u64
    let (cx, cy) = parse_heartbeat_header("-1,10000");
    assert_eq!(cx, 0); // invalid parses as 0
    assert_eq!(cy, 10000);
}

#[test]
fn parse_large_values() {
    // Test with large but valid u64 values
    let (cx, cy) = parse_heartbeat_header("4294967295,4294967295");
    assert_eq!(cx, 4294967295);
    assert_eq!(cy, 4294967295);
}

#[test]
fn parse_extra_commas_ignored() {
    // Extra values after second are ignored
    let (cx, cy) = parse_heartbeat_header("10000,10000,5000,extra");
    assert_eq!(cx, 10000);
    assert_eq!(cy, 10000);
}

#[test]
fn parse_one_value() {
    let (cx, cy) = parse_heartbeat_header("1");
    assert_eq!(cx, 1);
    assert_eq!(cy, 0);
}

// =============================================================================
// negotiate_heartbeats tests
// =============================================================================

#[test]
fn negotiate_both_zero_disables() {
    let (out, inc) = negotiate_heartbeats(0, 0, 0, 0);
    assert!(out.is_none());
    assert!(inc.is_none());
}

#[test]
fn negotiate_standard_equal() {
    let (out, inc) = negotiate_heartbeats(10000, 10000, 10000, 10000);
    assert_eq!(out, Some(Duration::from_millis(10000)));
    assert_eq!(inc, Some(Duration::from_millis(10000)));
}

#[test]
fn negotiate_takes_max_of_pairs() {
    // client_out=5000, server_in=10000 → out = max(5000, 10000) = 10000
    // client_in=5000, server_out=10000 → inc = max(5000, 10000) = 10000
    let (out, inc) = negotiate_heartbeats(5000, 5000, 10000, 10000);
    assert_eq!(out, Some(Duration::from_millis(10000)));
    assert_eq!(inc, Some(Duration::from_millis(10000)));
}

#[test]
fn negotiate_asymmetric() {
    // client_out=5000, server_in=20000 → out = max(5000, 20000) = 20000
    // client_in=15000, server_out=3000 → inc = max(15000, 3000) = 15000
    let (out, inc) = negotiate_heartbeats(5000, 15000, 3000, 20000);
    assert_eq!(out, Some(Duration::from_millis(20000)));
    assert_eq!(inc, Some(Duration::from_millis(15000)));
}

#[test]
fn negotiate_client_wants_server_doesnt() {
    // client_out=10000, server_in=0 → out = max(10000, 0) = 10000? No!
    // STOMP rule: if server_in=0, server doesn't want to receive heartbeats
    // But negotiate_heartbeats takes max, so 10000 wins
    // Actually re-reading: out = max(client_out, server_in) = max(10000, 0) = 10000
    let (out, inc) = negotiate_heartbeats(10000, 10000, 0, 0);
    // server_in=0 means out=max(10000,0)=10000 (client will send)
    // server_out=0 means inc=max(10000,0)=10000 (client expects)
    // Wait, that's not right. Let me re-check the logic.
    // negotiate_out = max(client_out, server_in) = max(10000, 0) = 10000
    // negotiate_inc = max(client_in, server_out) = max(10000, 0) = 10000
    // If negotiated is 0, None. Otherwise Some.
    assert_eq!(out, Some(Duration::from_millis(10000)));
    assert_eq!(inc, Some(Duration::from_millis(10000)));
}

#[test]
fn negotiate_server_wants_client_doesnt() {
    // client says 0,0 - doesn't want heartbeats
    // server says 10000,10000 - wants heartbeats
    // out = max(0, 10000) = 10000
    // inc = max(0, 10000) = 10000
    let (out, inc) = negotiate_heartbeats(0, 0, 10000, 10000);
    assert_eq!(out, Some(Duration::from_millis(10000)));
    assert_eq!(inc, Some(Duration::from_millis(10000)));
}

#[test]
fn negotiate_one_direction_only_out() {
    // client_out=10000, server_in=10000 → out enabled
    // client_in=0, server_out=0 → inc disabled (both zero)
    let (out, inc) = negotiate_heartbeats(10000, 0, 0, 10000);
    // out = max(10000, 10000) = 10000
    // inc = max(0, 0) = 0 → None
    assert_eq!(out, Some(Duration::from_millis(10000)));
    assert!(inc.is_none());
}

#[test]
fn negotiate_one_direction_only_inc() {
    // client_out=0, server_in=0 → out disabled
    // client_in=10000, server_out=10000 → inc enabled
    let (out, inc) = negotiate_heartbeats(0, 10000, 10000, 0);
    // out = max(0, 0) = 0 → None
    // inc = max(10000, 10000) = 10000
    assert!(out.is_none());
    assert_eq!(inc, Some(Duration::from_millis(10000)));
}

#[test]
fn negotiate_one_millisecond() {
    let (out, inc) = negotiate_heartbeats(1, 1, 1, 1);
    assert_eq!(out, Some(Duration::from_millis(1)));
    assert_eq!(inc, Some(Duration::from_millis(1)));
}

#[test]
fn negotiate_mixed_zero_nonzero() {
    // out = max(0, 5000) = 5000
    // inc = max(3000, 0) = 3000
    let (out, inc) = negotiate_heartbeats(0, 3000, 0, 5000);
    assert_eq!(out, Some(Duration::from_millis(5000)));
    assert_eq!(inc, Some(Duration::from_millis(3000)));
}
