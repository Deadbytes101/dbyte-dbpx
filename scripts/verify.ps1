$ErrorActionPreference = "Stop"

cargo fmt --check
cargo check
cargo test
