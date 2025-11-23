# iridium-stomp

[![CI](https://github.com/bsiegfreid/iridium-stomp/actions/workflows/ci.yml/badge.svg)](https://github.com/bsiegfreid/iridium-stomp/actions/workflows/ci.yml)

Asynchronous STOMP 1.2 client library for Rust.

## Quick start:

1. Add the crate to your project. If you are using the repository directly (not published on crates.io), add a dependency in your `Cargo.toml`:

```toml
[dependencies]
iridium-stomp = { git = "https://github.com/bsiegfreid/iridium-stomp", branch = "main" }
```

2. Run the provided smoke test against a local broker (convenient for validating your environment):

```bash
# Start a local test broker (see developer docs for docker-compose)
docker compose up -d

# Run the integration smoke test
cargo test --test stomp_smoke
```

## Minimal usage example

```rust
use iridium_stomp::{Connection, Frame};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	// Connect to a STOMP broker (addr, login, passcode, client-heartbeat)
	let mut conn = Connection::connect("127.0.0.1:61613", "guest", "guest", "10000,10000").await?;

	// Send a simple message to a destination
	let msg = Frame::new("SEND")
		.header("destination", "/queue/test")
		.set_body(b"hello from iridium-stomp".to_vec());

	conn.send_frame(msg).await?;

	// Read one incoming frame (if any)
	if let Some(frame) = conn.next_frame().await {
		println!("received frame: {}", frame);
	}

	// Close the connection (consumes the Connection)
	conn.close().await;

	Ok(())
}
```

## Running the quickstart example

To run the example added in `examples/quickstart.rs`:

1. Start a local STOMP-capable broker (RabbitMQ with the STOMP plugin is recommended):

```bash
docker compose up -d
```

2. From the crate directory, run the example:

```bash
cd iridium-stomp
cargo run --example quickstart
```

Notes:
- The example connects to `127.0.0.1:61613` using the `guest`/`guest` credentials by default.
- The example will time out waiting for an incoming frame after 5 seconds and exit cleanly if none arrives.
- If you want the example validated on CI, I can add a build-only job that runs `cargo build --examples`.

