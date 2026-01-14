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

```rust
use iridium_stomp::{Connection, Frame};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to a STOMP broker
    let conn = Connection::connect(
        "127.0.0.1:61613",
        "guest",
        "guest",
        "10000,10000"  // heartbeat: send every 10s, expect every 10s
    ).await?;

    // Send a message
    let msg = Frame::new("SEND")
        .header("destination", "/queue/test")
        .set_body(b"hello from iridium-stomp".to_vec());
    conn.send_frame(msg).await?;

    // Subscribe to a queue
    let mut subscription = conn
        .subscribe("/queue/test", iridium_stomp::connection::AckMode::Auto)
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

Heartbeats are negotiated automatically during connection. Pass your desired
intervals in the `heart-beat` format (`send,receive` in milliseconds):

```rust
// Client wants to send heartbeats every 10 seconds
// and receive them every 10 seconds
let conn = Connection::connect(addr, login, pass, "10000,10000").await?;
```

The library handles the negotiation (taking the maximum of client and server
preferences), sends heartbeats when the connection is idle, and closes the
connection if the server stops responding.

### Subscription Management

Subscribe to destinations with automatic resubscription on reconnect:

```rust
use iridium_stomp::connection::AckMode;

// Auto-acknowledge (server considers delivered immediately)
let sub = conn.subscribe("/queue/events", AckMode::Auto).await?;

// Client-acknowledge (cumulative)
let sub = conn.subscribe("/queue/jobs", AckMode::Client).await?;

// Client-individual (per-message acknowledgement)
let sub = conn.subscribe("/queue/tasks", AckMode::ClientIndividual).await?;
```

For broker-specific headers (durable subscriptions, selectors, etc.):

```rust
use iridium_stomp::SubscriptionOptions;

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

```rust
let conn = Connection::connect(...).await?;
let conn2 = conn.clone();

tokio::spawn(async move {
    conn2.send_frame(some_frame).await.unwrap();
});
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

```
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
