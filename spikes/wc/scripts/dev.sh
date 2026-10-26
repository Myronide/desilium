#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --workspace --locked
exec target/debug/wc-compositor --settings --command "$PWD/target/debug/wc-demo"
