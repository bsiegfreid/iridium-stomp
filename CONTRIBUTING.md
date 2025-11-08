Contributing
============

Thanks for wanting to contribute! This project aims to be small, well-tested, and easy to extend.

Quick developer guide
---------------------

This file contains the developer-facing instructions for running tests, examples, and CI-equivalent checks locally. If you are changing code, run these checks before opening a pull request.

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

- `Format check` — `cargo fmt --all -- --check`
- `Clippy` — `cargo clippy --all-targets --all-features -- -D warnings`
- `Unit tests` — runs `cargo test --lib` on a matrix of Rust toolchains (stable, beta, nightly)
- `Build examples` — `cargo build --examples`
- `test` — integration smoke test that builds a baked RabbitMQ image and runs `tests/stomp_smoke.rs`

Submitting patches
------------------

- Fork the repo and open a branch for your change.
- Keep commits small and focused.
- Add tests for new behavior when practical.
- Open a PR against `main` and reference any related issues.
