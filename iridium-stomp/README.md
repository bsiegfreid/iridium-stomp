# iridium-stomp

Asynchronous STOMP 1.2 client library for Rust — lightweight, async-first, and focused on heartbeat negotiation and reconnect behavior.

This README is the crate-level README used by Cargo and crates.io. For developer-facing instructions, CI, and helper scripts, see the repository root `README.md` and `CONTRIBUTING.md`.

Quickstart (crate)

Add the crate from GitHub for local testing:

```toml
[dependencies]
iridium-stomp = { git = "https://github.com/bsiegfreid/iridium-stomp", branch = "main" }
```

Run the example (from the repository root):

```bash
cd iridium-stomp
cargo run --example quickstart
```

License: MIT
