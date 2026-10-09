#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --quiet --release -p kairos-platform-macos --example probe_demo -- "${1:-60}"
