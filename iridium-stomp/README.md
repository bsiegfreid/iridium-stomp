# iridium-stomp

Asynchronous STOMP 1.2 client library for Rust — lightweight, async-first, and focused on heartbeat negotiation and reconnect behavior.

This README is the crate-level README used by Cargo and crates.io. For developer-facing instructions, CI, and helper scripts, see the repository root `README.md` and `CONTRIBUTING.md`.

## Quickstart (crate)

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

## Documentation

API and crate documentation is published to docs.rs after releases:

- docs.rs: <https://docs.rs/iridium-stomp>

For branch/nightly previews we publish the generated rustdoc to GitHub Pages on
push to `main` via CI; the site (when published) will be available at:

- GitHub Pages: <https://bsiegfreid.github.io/iridium-stomp/>

If you want to preview the docs locally before publishing, run:

```bash
cargo doc --manifest-path iridium-stomp/Cargo.toml --no-deps
# open target/doc/iridium_stomp/index.html in your browser
```
