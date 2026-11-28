#!/usr/bin/env bash
# Build ungive/mediaremote-adapter (BSD-3-Clause) into src-tauri/macos/ so the macOS app can
# read the system "Now Playing" info. Run on macOS before `tauri build` or `cargo build`.
# Needs git, cmake and Xcode command line tools.
set -euo pipefail

TAG="v0.9.1"
COMMIT_PREFIX="ca788dc"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/src-tauri/macos"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git clone --quiet --depth 1 --branch "$TAG" https://github.com/ungive/mediaremote-adapter.git "$WORK/src"
HEAD="$(git -C "$WORK/src" rev-parse HEAD)"
if [[ "$HEAD" != "$COMMIT_PREFIX"* ]]; then
  echo "mediaremote-adapter $TAG resolved to $HEAD, expected $COMMIT_PREFIX..." >&2
  exit 1
fi

cmake -S "$WORK/src" -B "$WORK/build" -DCMAKE_BUILD_TYPE=Release
cmake --build "$WORK/build" --config Release

rm -rf "$OUT"
mkdir -p "$OUT"
# -R keeps the framework's internal symlinks (Versions/Current) intact.
cp -R "$WORK/build/MediaRemoteAdapter.framework" "$OUT/"
cp "$WORK/src/bin/mediaremote-adapter.pl" "$OUT/"
cp "$WORK/src/LICENSE" "$OUT/MEDIAREMOTE-ADAPTER-LICENSE"
echo "mediaremote-adapter $TAG ($HEAD) built into $OUT"
