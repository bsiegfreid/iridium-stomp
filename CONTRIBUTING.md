Contributing
============

Thanks for wanting to contribute! This project aims to be small, well-tested, and easy to extend.

Quick developer guide
---------------------

This file contains the developer-facing instructions for running tests, examples, and CI-equivalent checks locally. If you are changing code, run these checks before opening a pull request.

### Pre-submission checklist

**Before submitting a PR, always run these commands** from the repository root:

```bash
# 1. Format code (this will auto-fix formatting issues)
cargo fmt --all

# 2. Verify formatting is correct
cargo fmt --all -- --check

# 3. Run lints (CI uses -D warnings)
cargo clippy --all-targets --all-features -- -D warnings

# 4. Run unit tests
cargo test --lib

# 5. Build examples (quick sanity check)
cargo build --examples
```

Optional but recommended:

```bash
# Run the quickstart example (optional)
cargo run --example quickstart

# Build and run the CLI (optional)
cargo run --features cli --bin stomp -- --help
```

Running RabbitMQ locally (developer helper)
------------------------------------------

A lightweight RabbitMQ instance with the STOMP plugin is provided via `docker-compose.yml` at the repository root. This is intended for local development and tests.

Quick start:

```bash
docker compose up -d
```

Check status and logs:

```bash
docker compose ps
docker compose logs -f rabbitmq
```

Stop and remove the container and data volume:

```bash
docker compose down -v
```

Helper script
-------------

There is a helper script at `scripts/test-with-rabbit.sh` that builds a RabbitMQ image with the STOMP plugin baked in, runs it locally, waits for STOMP to be ready, runs the Rust smoke test, and cleans up.

Usage (from the repository root):

```bash
./scripts/test-with-rabbit.sh
```

CI job summary
--------------

The GitHub Actions workflow runs the following jobs (see `.github/workflows/ci.yml`):

- `Smoke integration test` — Builds a RabbitMQ container with STOMP, waits for full readiness using management API checks, and runs `tests/stomp_smoke.rs` to verify end-to-end connectivity
- `Format check` — `cargo fmt --all -- --check`
- `Clippy` — `cargo clippy --all-targets --all-features -- -D warnings`
- `Unit tests` — runs `cargo test --lib` on a matrix of Rust toolchains (stable, beta, nightly)
- `Build examples` — `cargo build --examples`

**Smoke test robustness**: The integration test uses a multi-stage readiness check to prevent flaky failures:
1. Management API health check (broker is starting)
2. STOMP plugin enabled verification (plugin is operational)
3. STOMP port connection test (port accepts connections)
4. Test itself includes retry logic with exponential backoff

This approach ensures the broker is genuinely ready before tests run, eliminating race conditions that cause intermittent CI failures.

Submitting patches
------------------

- Fork the repo and open a branch for your change.
- Keep commits small and focused.
- Add tests for new behavior when practical.
- Open a PR against `main` and reference any related issues.

## Parser architecture (STOMP codec)

This project parses STOMP frames with a small, focused parser invoked from the codec on the current input slice. Goals remain: correctness under arbitrary TCP chunking, robust handling of large/binary bodies, and clear incremental semantics.

Why this approach
- Avoids fragile index arithmetic and manual buffer-management bugs that arise when callers feed partially-complete frames in arbitrary splits.
- Keeps parsing code small and testable: the codec calls a single parser function on `src.chunk()` and advances the `BytesMut` by the parser-consumed offset on success.
- Supports both zero-copy header/command observation and owned/returned bodies for downstream processing.

Key concepts
- Parser implementation: the crate currently uses a simple slice-based parser `parse_frame_slice(input: &[u8])` implemented in `src/parser.rs`. It accepts the input slice and returns either:
  - `Ok(None)` when more bytes are needed (incomplete),
  - `Ok(Some((command, headers, body_opt, consumed_bytes)))` on a successfully-parsed frame, or
  - `Err(String)` on a protocol-level parse error (for example: invalid Content-Length value).
- Offset/advance: the codec passes `src.chunk()` to the parser; when the parser returns a consumed byte count the codec calls `src.advance(consumed)` to drop parsed bytes and leave the buffer ready for the next frame.
- Body modes: STOMP bodies can be length-delimited (Content-Length header) or NUL-terminated. The parser handles both modes and accepts an optional trailing LF after the terminal NUL.
- Heartbeat frames: a single LF (0x0A) is treated as a heartbeat and handled as a short special-case frame by the codec.

Benefits vs alternatives
- Stateless ad-hoc parsing (index arithmetic): faster to prototype but brittle under many chunk-split patterns and incremental feeds — we observed missing-frame bugs in stress tests.
- Stateful internal-buffer decoder (owning buffer + state machine): can be robust but increases complexity (state transitions, reallocation, and harder-to-test behaviour). The slice-based parser keeps the decoder code simple while remaining robust.

Testing and reproduction
- Use the included unit tests that feed frames in small increments and random splits. Key tests:
  - `cargo test minimized_regression -- --nocapture`
  - `cargo test regression_replay -- --nocapture`
  - `cargo test -- --test-threads=1` (optional: serial run for deterministic output)
- When you capture a failing chunk sequence, add it under `tests/` as a replay file (see existing `tests/reducer_minimize.rs` and `tests/minimized_regression.rs`). The slice-based parser makes it straightforward to feed deterministic slices and reproduce chunking issues.
