#!/bin/sh
set -eux
: "${TEST_DATABASE_URL:?TEST_DATABASE_URL is required; integration tests must not be skipped}"
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -- --test-threads=4
sh scripts/audit.sh
