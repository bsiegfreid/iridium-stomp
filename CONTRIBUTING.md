# Contributing

## Quick developer guide

This file contains the developer-facing instructions for running tests,
examples, and CI-equivalent checks locally. If you are changing code, run these
checks before opening a pull request.

Run these commands from the crate directory:

```bash
# In the crate directory
cd iridium-stomp

# Formatting
cargo fmt --all -- --check

# Lints
cargo clippy --all-targets --all-features

# Unit tests
cargo test --lib

# Build examples (quick sanity check)
cargo build --examples

# Run the quickstart example (optional)
cargo run --example quickstart
```

## Running RabbitMQ locally

A lightweight RabbitMQ instance with the STOMP plugin is provided via
`docker-compose.yml` at the repository root. This is intended for local
development and tests.

Quick start:

```bash
docker compose up -d
```

Check status and logs:

```bash
docker compose ps
```

## Helper script

There is a helper script at `scripts/test-with-rabbit.sh` that builds a
RabbitMQ image with the STOMP plugin baked in, runs it locally, waits for STOMP
to be ready, runs the Rust smoke test, and cleans up.

```bash
./scripts/test-with-rabbit.sh
```

## Parser architecture (STOMP codec)

This project parses STOMP frames with a small, focused parser invoked from the
codec on the current input slice. Goals remain: correctness under arbitrary TCP
chunking, robust handling of large/binary bodies, and clear incremental
semantics.

### Why this approach

Avoids fragile index arithmetic and manual buffer-management bugs that arise
when callers feed partially-complete frames in arbitrary splits.

Keeps parsing code small and testable: the codec calls a single parser
function on `src.chunk()` and advances the `BytesMut` by the parser-consumed
offset on success.

Supports both zero-copy header/command observation and owned/returned bodies
for downstream processing.

### Key concepts

- Parser implementation: the crate currently uses a simple slice-based parser
`parse_frame_slice(input: &[u8])` implemented in `src/parser.rs`. It accepts
the input slice and returns either:
	- `Ok(None)` when more bytes are needed (incomplete),
	- `Ok(Some((command, headers, body_opt, consumed_bytes)))` on a
	successfully-parsed frame, or
	- `Err(String)` on a protocol-level parse error (for example: invalid
	Content-Length value).
- Offset/advance: the codec passes `src.chunk()` to the parser; when the parser
returns a consumed byte count the codec calls `src.advance(consumed)` to drop
parsed bytes and leave the buffer ready for the next frame.
- Body modes: STOMP bodies can be length-delimited (Content-Length header) or
NUL-terminated. The parser handles both modes and accepts an optional trailing
LF after the terminal NUL.
- Heartbeat frames: a single LF (0x0A) is treated as a heartbeat and handled as
a short special-case frame by the codec.

### Testing and reproduction

Use the included unit tests that feed frames in small increments and random
splits.

```
cargo test minimized_regression
cargo test regression_replay
```
