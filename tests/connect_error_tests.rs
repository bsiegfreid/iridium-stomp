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

/// Test that server closing connection before CONNECTED returns Protocol error
#[tokio::test]
async fn connect_closed_before_connected_returns_protocol_error() {
    let port = get_available_port();
    let addr = format!("127.0.0.1:{}", port);

    // Spawn a mock server that closes immediately after receiving CONNECT
    let server_addr = addr.clone();
    let server = thread::spawn(move || {
        let listener = TcpListener::bind(&server_addr).unwrap();
        listener.set_nonblocking(false).unwrap();

        if let Ok((mut stream, _)) = listener.accept() {
            // Read the CONNECT frame
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            // Close without sending CONNECTED
            drop(stream);
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

    // Attempt connection
    let result = Connection::connect(&addr, "user", "pass", "0,0").await;

    // Should get Protocol error about connection closed
    match result {
        Err(ConnError::Protocol(msg)) => {
            assert!(
                msg.contains("closed") || msg.contains("CONNECTED"),
                "Expected message about connection closed, got: {}",
                msg
            );
        }
        Err(ConnError::Io(_)) => {
            // Also acceptable - connection reset
        }
        Err(other) => panic!("Expected Protocol or Io error, got: {:?}", other),
        Ok(_) => panic!("Expected error, got successful connection"),
    }

    server.join().unwrap();
}

/// Test that connection refused returns Io error
#[tokio::test]
async fn connect_refused_returns_io_error() {
    // Use a port that nothing is listening on
    let port = get_available_port();
    let addr = format!("127.0.0.1:{}", port);

    // Don't start any server - port should be closed

    let result = Connection::connect(&addr, "user", "pass", "0,0").await;

    match result {
        Err(ConnError::Io(err)) => {
            assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused);
        }
        Err(other) => panic!("Expected Io error, got: {:?}", other),
        Ok(_) => panic!("Expected error, got successful connection"),
    }
}
