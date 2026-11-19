#!/usr/bin/env bash
# Build the optimized release binary and place it in dist/amo-monitor.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

echo "==> Building release binary (target: $(uname -m)-unknown-linux-gnu)"
cargo build --release

mkdir -p dist
install -m 0755 target/release/amo-monitor dist/amo-monitor

echo "==> Done: $ROOT/dist/amo-monitor"
"$ROOT/dist/amo-monitor" --version || true
