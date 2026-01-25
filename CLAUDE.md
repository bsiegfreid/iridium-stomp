# Project Rules for Claude

No git add, commit, or push

## Before code is considered ready for commits

Format all code for consistency
```
cargo fmt --all
```

Verify formatting is correct
```
cargo fmt --all -- --check
```

Run lints
```
cargo clippy --all-targets --all-features -- -D warnings
```

Run unit tests
```
cargo test --lib
```

