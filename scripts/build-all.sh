#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TARGETS=(
  "aarch64-apple-darwin"
  "x86_64-apple-darwin"
  "x86_64-unknown-linux-gnu"
  "aarch64-unknown-linux-gnu"
  "x86_64-pc-windows-msvc"
)

for target in "${TARGETS[@]}"; do
  echo "==> building $target"
  "$PROJECT_ROOT/scripts/build-release.sh" "$target"
done
