#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <target>"
  exit 1
fi

TARGET="$1"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
BINARY_NAME="pyrunner"

mkdir -p "$DIST_DIR"

build_with_cargo() {
  cargo build --release --target "$TARGET"
}

build_with_zigbuild() {
  cargo zigbuild --release --target "$TARGET"
}

build_with_xwin() {
  cargo xwin build --release --target "$TARGET"
}

case "$TARGET" in
  x86_64-pc-windows-msvc)
    if command -v cargo-xwin >/dev/null 2>&1 || cargo xwin --help >/dev/null 2>&1; then
      build_with_xwin
    else
      build_with_cargo
    fi
    ARTIFACT="$PROJECT_ROOT/target/$TARGET/release/${BINARY_NAME}.exe"
    OUTPUT="$DIST_DIR/${BINARY_NAME}-${TARGET}.exe"
    ;;
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-apple-darwin|aarch64-apple-darwin)
    if command -v cargo-zigbuild >/dev/null 2>&1 || cargo zigbuild --help >/dev/null 2>&1; then
      build_with_zigbuild
    else
      build_with_cargo
    fi
    ARTIFACT="$PROJECT_ROOT/target/$TARGET/release/${BINARY_NAME}"
    OUTPUT="$DIST_DIR/${BINARY_NAME}-${TARGET}"
    ;;
  *)
    build_with_cargo
    ARTIFACT="$PROJECT_ROOT/target/$TARGET/release/${BINARY_NAME}"
    OUTPUT="$DIST_DIR/${BINARY_NAME}-${TARGET}"
    ;;
esac

cp "$ARTIFACT" "$OUTPUT"
echo "built: $OUTPUT"
