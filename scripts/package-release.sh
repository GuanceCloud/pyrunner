#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"

mkdir -p "$DIST_DIR"

shopt -s nullglob
for artifact in "$DIST_DIR"/pyrunner-*; do
  base="$(basename "$artifact")"
  if [[ "$base" == *.tar.gz || "$base" == *.zip ]]; then
    continue
  fi

  if [[ "$base" == *.exe ]]; then
    zip -j "$DIST_DIR/${base}.zip" "$artifact"
  else
    tar -czf "$DIST_DIR/${base}.tar.gz" -C "$DIST_DIR" "$base"
  fi
done

echo "packaged artifacts in $DIST_DIR"
