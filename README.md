# iridium-stomp

[![CI](https://github.com/bsiegfreid/iridium-stomp/actions/workflows/ci.yml/badge.svg)](https://github.com/bsiegfreid/iridium-stomp/actions/workflows/ci.yml)

An asynchronous STOMP 1.2 client library for Rust.

> **Early Development**: This library is heavily tested (150+ unit and fuzz tests) but has not yet been battle-tested in production environments. APIs may change. Use with appropriate caution.

## Motivation

STOMP is deceptively simple on the surface—text-based frames, straightforward
commands, easy to read with `nc` or `telnet`. But the details matter:

- **Heartbeats** need to be negotiated correctly, or your connection dies
  unexpectedly
- **TCP chunking** means frames can arrive in arbitrary pieces, and your parser
  better handle that gracefully
- **Binary bodies** with embedded NUL bytes require content-length headers,
  which many implementations get wrong
- **Reconnection** should be automatic and transparent, not something you have
  to build yourself

I wanted a library that handled all of this correctly, without me having to
think about it every time I wrote application code.

## Design Goals

- **Async-first architecture** — Built on Tokio from the ground up, not bolted
  on as an afterthought.

- **Correct frame parsing** — Handles arbitrary TCP chunk boundaries, binary
  bodies with embedded NULs, and the full STOMP 1.2 frame format.

- **Automatic heartbeat management** — Negotiates heartbeat intervals per the
  spec, sends heartbeats when idle, and detects missed heartbeats from the
  server.

- **Transparent reconnection** — Exponential backoff, automatic resubscription,
  and pending message cleanup on disconnect.

- **Small, explicit API** — One way to do things, clearly documented, easy to
  understand.

- **Production-ready testing** — 150+ tests including fuzz testing, stress
  testing, and regression capture for previously-failing edge cases.

## Non-Goals

There are some things this library intentionally does *not* try to be:

- A full STOMP server implementation
- A message queue abstraction layer
- Compatible with STOMP versions prior to 1.2
- A broker-specific client (ActiveMQ extensions, RabbitMQ-specific features)

iridium-stomp is a STOMP 1.2 client library. If you need broker-specific
features, you can pass custom headers through `subscribe_with_headers` or
`SubscriptionOptions`, but the library itself stays protocol-focused.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
iridium-stomp = { git = "https://github.com/bsiegfreid/iridium-stomp", branch = "main" }
```

## Quick Start

```rust,no_run
use iridium_stomp::{Connection, Frame, ReceivedFrame};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to a STOMP broker
    let conn = Connection::connect(
        "127.0.0.1:61613",
        "guest",
        "guest",
        Connection::DEFAULT_HEARTBEAT,  // 10 seconds send/receive
    ).await?;

    // Send a message
    let msg = Frame::new("SEND")
        .header("destination", "/queue/test")
        .set_body(b"hello from iridium-stomp".to_vec());
    conn.send_frame(msg).await?;

    // Subscribe to a queue
    let mut subscription = conn
        .subscribe("/queue/test", iridium_stomp::AckMode::Auto)
        .await?;

    // Receive messages using the Stream trait
    use futures::StreamExt;
    while let Some(frame) = subscription.next().await {
        println!("Received: {:?}", frame);
    }

    conn.close().await;
    Ok(())
}
```

## Features

### Heartbeat Negotiation

Heartbeats are negotiated automatically during connection. Use the provided
constants or the `Heartbeat` struct for type-safe configuration:

```rust,ignore
use iridium_stomp::{Connection, Heartbeat};

// Use predefined constants
let conn = Connection::connect(addr, login, pass, Connection::DEFAULT_HEARTBEAT).await?;
let conn = Connection::connect(addr, login, pass, Connection::NO_HEARTBEAT).await?;

// Or use the Heartbeat struct for custom intervals
let hb = Heartbeat::new(5000, 10000);  // send every 5s, expect every 10s
let conn = Connection::connect(addr, login, pass, &hb.to_string()).await?;

// Create from Duration for symmetric intervals
use std::time::Duration;
let hb = Heartbeat::from_duration(Duration::from_secs(15));
```

The library handles the negotiation (taking the maximum of client and server
preferences), sends heartbeats when the connection is idle, and closes the
connection if the server stops responding.

### Subscription Management

Subscribe to destinations with automatic resubscription on reconnect:

```rust,ignore
use iridium_stomp::connection::AckMode;

// Auto-acknowledge (server considers delivered immediately)
let sub = conn.subscribe("/queue/events", AckMode::Auto).await?;

// Client-acknowledge (cumulative)
let sub = conn.subscribe("/queue/jobs", AckMode::Client).await?;

// Client-individual (per-message acknowledgement)
let sub = conn.subscribe("/queue/tasks", AckMode::ClientIndividual).await?;
```

For broker-specific headers (durable subscriptions, selectors, etc.):

```rust,ignore
use iridium_stomp::SubscriptionOptions;
use iridium_stomp::connection::AckMode;

let options = SubscriptionOptions {
    headers: vec![
        ("activemq.subscriptionName".into(), "my-durable-sub".into()),
        ("selector".into(), "priority > 5".into()),
    ],
    durable_queue: None,
};

let sub = conn.subscribe_with_options("/topic/events", AckMode::Client, options).await?;
```

### Cloneable Connection

The `Connection` is cloneable and thread-safe. Multiple tasks can share the
same connection:

```rust,ignore
let conn = Connection::connect(...).await?;
let conn2 = conn.clone();

tokio::spawn(async move {
    conn2.send_frame(some_frame).await.unwrap();
});
```

### Custom CONNECT Headers

Use `ConnectOptions` to customize the STOMP CONNECT frame for broker-specific
requirements like durable subscriptions or virtual hosts:

```rust,ignore
use iridium_stomp::{Connection, ConnectOptions};

let options = ConnectOptions::new()
    .client_id("my-durable-client")     // Required for ActiveMQ durable subscriptions
    .host("/production")                 // Virtual host (RabbitMQ)
    .accept_version("1.1,1.2")          // Version negotiation
    .header("custom-key", "value");     // Broker-specific headers

let conn = Connection::connect_with_options(
    "localhost:61613",
    "guest",
    "guest",
    Connection::DEFAULT_HEARTBEAT,
    options,
).await?;
```

### Receipt Confirmation

Request delivery confirmation from the broker using RECEIPT frames:

```rust,ignore
use iridium_stomp::{Connection, Frame};
use std::time::Duration;

let msg = Frame::new("SEND")
    .header("destination", "/queue/important")
    .receipt("msg-123")  // Request receipt with this ID
    .set_body(b"critical data".to_vec());

// Send and wait for confirmation (with timeout)
conn.send_frame_confirmed(msg, Duration::from_secs(5)).await?;

// Or handle receipts manually
let msg = Frame::new("SEND")
    .header("destination", "/queue/test")
    .receipt("msg-456")
    .set_body(b"data".to_vec());
conn.send_frame_with_receipt(msg).await?;
conn.wait_for_receipt("msg-456", Duration::from_secs(5)).await?;
```

### Connection Error Handling

Connection failures (invalid credentials, server unreachable) are reported immediately:

```rust,ignore
use iridium_stomp::Connection;
use iridium_stomp::connection::ConnError;

match Connection::connect("localhost:61613", "user", "pass", Connection::DEFAULT_HEARTBEAT).await {
    Ok(conn) => {
        // Connected successfully
    }
    Err(ConnError::ServerRejected(err)) => {
        // Authentication failed or server rejected connection
        eprintln!("Server rejected: {}", err.message);
    }
    Err(ConnError::Io(err)) => {
        // Network error (connection refused, timeout, etc.)
        eprintln!("Network error: {}", err);
    }
    Err(err) => {
        eprintln!("Connection failed: {}", err);
    }
}
```

### Server Error Handling

Errors received after connection are surfaced as `ReceivedFrame::Error`:

```rust,ignore
use iridium_stomp::{Connection, ReceivedFrame};

while let Some(received) = conn.next_frame().await {
    match received {
        ReceivedFrame::Frame(frame) => {
            println!("Got {}: {:?}", frame.command, frame.get_header("destination"));
        }
        ReceivedFrame::Error(err) => {
            eprintln!("Server error: {}", err.message);
            if let Some(body) = &err.body {
                eprintln!("Details: {}", body);
            }
            break;
        }
    }
}
```

## CLI

An interactive CLI is included for testing and ad-hoc messaging. Install with
the `cli` feature:

```bash
cargo install iridium-stomp --features cli
```

Or run from source:

```bash
cargo run --features cli --bin stomp -- --help
```

### CLI Usage

```bash
# Connect and subscribe to a queue
stomp -a 127.0.0.1:61613 -s /queue/test

# Connect with custom credentials
stomp -a broker.example.com:61613 -l myuser -p mypass -s /queue/events

# Subscribe to multiple queues
stomp -s /queue/orders -s /queue/notifications
```

Interactive commands:

```text
> send /queue/test Hello, World!
Sent to /queue/test

> sub /queue/other
Subscribed to: /queue/other

> help
Commands:
  send <destination> <message>  - Send a message
  sub <destination>             - Subscribe to a destination
  quit                          - Exit

> quit
Disconnecting...
```

## Running the Examples

Start a local STOMP broker (RabbitMQ with STOMP plugin):

```bash
docker compose up -d
```

Run the quickstart example:

```bash
cargo run --example quickstart
```

## Testing

The library includes comprehensive tests:

```bash
# Run all tests
cargo test

# Run specific test suites
cargo test --test heartbeat_unit    # Heartbeat parsing/negotiation
cargo test --test codec_heartbeat   # Wire format encoding/decoding
cargo test --test parser_unit       # Frame parsing edge cases
cargo test --test codec_fuzz        # Randomized chunk splitting
cargo test --test codec_stress      # Concurrent stress testing
```

### Integration Tests in CI

The CI workflow includes a smoke integration test that verifies the library works against a real RabbitMQ broker with STOMP enabled. This test ensures end-to-end functionality beyond unit tests.

**How it works:**

1. **Broker Setup**: CI builds a Docker image with RabbitMQ 3.11 and the STOMP plugin pre-enabled (see `.github/docker/rabbitmq-stomp/Dockerfile`)

2. **Readiness Checks**: Before running tests, CI performs multi-stage readiness verification:
   - Waits for RabbitMQ management API to respond (indicates broker is starting)
   - Verifies STOMP plugin is fully enabled via the management API
   - Confirms STOMP port 61613 accepts TCP connections
   
   This ensures the broker is truly ready, preventing flaky test failures from timing issues.

3. **Smoke Test**: Runs `tests/stomp_smoke.rs` which:
   - Attempts a STOMP CONNECT with retry logic (5 attempts with backoff)
   - Verifies the broker responds with CONNECTED frame
   - Reports detailed connection diagnostics on failure

4. **Debugging**: If tests fail, CI automatically dumps RabbitMQ logs for troubleshooting

**Running integration tests locally:**

Use the provided helper script which mimics the CI workflow:

```bash
./scripts/test-with-rabbit.sh
```

Or manually with docker-compose:

```bash
# Start RabbitMQ with STOMP
docker compose up -d

# Wait for it to be ready (management UI at http://localhost:15672)
# Then run the smoke test
RUN_STOMP_SMOKE=1 cargo test --test stomp_smoke

# Cleanup
docker compose down -v
```

The smoke test is skipped by default unless `RUN_STOMP_SMOKE=1` is set, since it requires an external broker.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, running tests
locally, and CI information.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for
details.

## About iridiumdesign

Iridiumdesign—and the iridiumdesign.com domain—started back in 2000 while I was
finishing design school. At the time, it was meant to support freelance work in
graphic design and web development.

Over the years, as I moved into full-time corporate software engineering,
Iridiumdesign became less of a business and more of a sandbox. It's where I
experiment, learn, and build things that don't always fit neatly into my day
job.

These days I'm a senior software engineer and don't do much design work
anymore, but the *iridium* name stuck. I use it as a prefix for my personal
libraries and projects so they're easy to identify and group together.

iridium-stomp is one of those projects: something I built because I needed it,
learned from, and decided was worth sharing.

*Brad Siegfreid*
