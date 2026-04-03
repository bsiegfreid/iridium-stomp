//! Tests for connection error handling during CONNECT phase.
//!
//! These tests verify that ERROR frames and connection failures during
//! the STOMP handshake are properly reported to the caller.

use iridium_stomp::Connection;
use iridium_stomp::connection::ConnError;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

/// Helper to find an available port
fn get_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Test that server sending ERROR frame during CONNECT returns ServerRejected
#[tokio::test]
async fn connect_error_frame_returns_server_rejected() {
    let port = get_available_port();
    let addr = format!("127.0.0.1:{}", port);

    // Spawn a mock server that sends an ERROR frame
    let server_addr = addr.clone();
    let server = thread::spawn(move || {
        let listener = TcpListener::bind(&server_addr).unwrap();
        listener.set_nonblocking(false).unwrap();

        if let Ok((mut stream, _)) = listener.accept() {
            // Read the CONNECT frame (we don't need to parse it fully)
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            // Send ERROR frame
            let error_frame = "ERROR\nmessage:Authentication failed\n\nInvalid credentials\0";
            stream.write_all(error_frame.as_bytes()).unwrap();
            stream.flush().unwrap();

            // Keep connection open briefly so client can read
            thread::sleep(Duration::from_millis(100));
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

    // Attempt connection
    let result = Connection::connect(&addr, "user", "wrongpass", "0,0").await;

    // Should get ServerRejected error
    match result {
        Err(ConnError::ServerRejected(err)) => {
            assert_eq!(err.message, "Authentication failed");
            assert_eq!(err.body, Some("Invalid credentials".to_string()));
        }
        Err(other) => panic!("Expected ServerRejected, got: {:?}", other),
        Ok(_) => panic!("Expected error, got successful connection"),
    }

    server.join().unwrap();
}

/// Test that server closing connection before CONNECTED causes a retry
/// (not an immediate failure). Protocol errors during handshake are transient.
#[tokio::test]
async fn connect_closed_before_connected_retries() {
    let port = get_available_port();
    let addr = format!("127.0.0.1:{}", port);

    // Spawn a mock server that closes immediately after receiving CONNECT
    let server_addr = addr.clone();
    let server = thread::spawn(move || {
        let listener = TcpListener::bind(&server_addr).unwrap();
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);
                    drop(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

    // Should keep retrying, not return an error
    let result = tokio::time::timeout(
        Duration::from_millis(500),
        Connection::connect(&addr, "user", "pass", "0,0"),
    )
    .await;

    assert!(
        result.is_err(),
        "Expected connect to keep retrying when server closes during handshake"
    );

    server.join().unwrap();
}

/// Test that connection refused retries (does not fail immediately).
///
/// With initial connection retry, an unreachable broker causes `connect` to
/// retry with backoff. We verify it does *not* return within a short window,
/// then cancel the attempt.
#[tokio::test]
async fn connect_refused_retries_instead_of_failing() {
    let port = get_available_port();
    let addr = format!("127.0.0.1:{}", port);

    // No server listening — connect should retry, not return an error.
    let result = tokio::time::timeout(
        Duration::from_millis(500),
        Connection::connect(&addr, "user", "pass", "0,0"),
    )
    .await;

    // Timeout means connect is still retrying — expected behaviour.
    assert!(
        result.is_err(),
        "Expected connect to keep retrying, but it returned"
    );
}
