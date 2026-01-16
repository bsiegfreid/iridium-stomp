//! Tests for ConnectOptions and connect_with_options (Issue #34)
//!
//! These tests verify:
//! - ConnectOptions builder methods
//! - Default values
//! - Custom headers

use iridium_stomp::ConnectOptions;

// ============================================================================
// ConnectOptions builder tests
// ============================================================================

#[test]
fn connect_options_default() {
    let opts = ConnectOptions::default();
    assert!(opts.accept_version.is_none());
    assert!(opts.client_id.is_none());
    assert!(opts.host.is_none());
    assert!(opts.headers.is_empty());
}

#[test]
fn connect_options_new() {
    let opts = ConnectOptions::new();
    assert!(opts.accept_version.is_none());
    assert!(opts.client_id.is_none());
    assert!(opts.host.is_none());
    assert!(opts.headers.is_empty());
}

#[test]
fn connect_options_accept_version() {
    let opts = ConnectOptions::default().accept_version("1.0,1.1,1.2");
    assert_eq!(opts.accept_version, Some("1.0,1.1,1.2".to_string()));
}

#[test]
fn connect_options_accept_version_single() {
    let opts = ConnectOptions::default().accept_version("1.1");
    assert_eq!(opts.accept_version, Some("1.1".to_string()));
}

#[test]
fn connect_options_client_id() {
    let opts = ConnectOptions::default().client_id("my-durable-client");
    assert_eq!(opts.client_id, Some("my-durable-client".to_string()));
}

#[test]
fn connect_options_client_id_with_string() {
    let id = String::from("client-123");
    let opts = ConnectOptions::default().client_id(id);
    assert_eq!(opts.client_id, Some("client-123".to_string()));
}

#[test]
fn connect_options_host() {
    let opts = ConnectOptions::default().host("my-vhost");
    assert_eq!(opts.host, Some("my-vhost".to_string()));
}

#[test]
fn connect_options_host_with_slash() {
    let opts = ConnectOptions::default().host("/production");
    assert_eq!(opts.host, Some("/production".to_string()));
}

#[test]
fn connect_options_header_single() {
    let opts = ConnectOptions::default().header("custom-key", "custom-value");
    assert_eq!(opts.headers.len(), 1);
    assert_eq!(
        opts.headers[0],
        ("custom-key".to_string(), "custom-value".to_string())
    );
}

#[test]
fn connect_options_header_multiple() {
    let opts = ConnectOptions::default()
        .header("key1", "value1")
        .header("key2", "value2")
        .header("key3", "value3");
    assert_eq!(opts.headers.len(), 3);
    assert_eq!(opts.headers[0], ("key1".to_string(), "value1".to_string()));
    assert_eq!(opts.headers[1], ("key2".to_string(), "value2".to_string()));
    assert_eq!(opts.headers[2], ("key3".to_string(), "value3".to_string()));
}

#[test]
fn connect_options_builder_chain() {
    let opts = ConnectOptions::default()
        .accept_version("1.2")
        .client_id("test-client")
        .host("test-vhost")
        .header("x-custom", "value");

    assert_eq!(opts.accept_version, Some("1.2".to_string()));
    assert_eq!(opts.client_id, Some("test-client".to_string()));
    assert_eq!(opts.host, Some("test-vhost".to_string()));
    assert_eq!(opts.headers.len(), 1);
    assert_eq!(
        opts.headers[0],
        ("x-custom".to_string(), "value".to_string())
    );
}

#[test]
fn connect_options_clone() {
    let opts1 = ConnectOptions::default()
        .client_id("original")
        .host("vhost1");
    let opts2 = opts1.clone();

    assert_eq!(opts1.client_id, opts2.client_id);
    assert_eq!(opts1.host, opts2.host);
}

#[test]
fn connect_options_debug() {
    let opts = ConnectOptions::default().client_id("test");
    let debug = format!("{:?}", opts);
    assert!(debug.contains("ConnectOptions"));
    assert!(debug.contains("test"));
}

// ============================================================================
// ConnectOptions with special values
// ============================================================================

#[test]
fn connect_options_empty_client_id() {
    // Empty client-id is technically valid (though likely not useful)
    let opts = ConnectOptions::default().client_id("");
    assert_eq!(opts.client_id, Some(String::new()));
}

#[test]
fn connect_options_empty_host() {
    let opts = ConnectOptions::default().host("");
    assert_eq!(opts.host, Some(String::new()));
}

#[test]
fn connect_options_header_empty_value() {
    let opts = ConnectOptions::default().header("key", "");
    assert_eq!(opts.headers[0], ("key".to_string(), String::new()));
}

#[test]
fn connect_options_header_empty_key() {
    // Empty key is technically allowed by the builder
    let opts = ConnectOptions::default().header("", "value");
    assert_eq!(opts.headers[0], (String::new(), "value".to_string()));
}

#[test]
fn connect_options_activemq_durable() {
    // Typical ActiveMQ durable subscription setup
    let opts = ConnectOptions::default()
        .client_id("my-app-subscriber-1")
        .header("activemq.prefetchSize", "1");

    assert_eq!(opts.client_id, Some("my-app-subscriber-1".to_string()));
    assert_eq!(opts.headers.len(), 1);
    assert_eq!(
        opts.headers[0],
        ("activemq.prefetchSize".to_string(), "1".to_string())
    );
}

#[test]
fn connect_options_rabbitmq_vhost() {
    // RabbitMQ virtual host example
    let opts = ConnectOptions::default().host("/production");

    assert_eq!(opts.host, Some("/production".to_string()));
}

#[test]
fn connect_options_version_negotiation_fallback() {
    // Client willing to accept multiple versions for compatibility
    let opts = ConnectOptions::default().accept_version("1.0,1.1,1.2");

    assert_eq!(opts.accept_version, Some("1.0,1.1,1.2".to_string()));
}

#[test]
fn connect_options_multiple_custom_headers() {
    // Some brokers accept multiple custom headers
    let opts = ConnectOptions::default()
        .header("x-request-id", "uuid-12345")
        .header("x-correlation-id", "corr-67890")
        .header("x-tenant", "tenant-abc");

    assert_eq!(opts.headers.len(), 3);
}
