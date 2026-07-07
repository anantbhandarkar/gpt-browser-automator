#!/usr/bin/env bash
# Build gptbrowser (release) and install it onto PATH.
#
# Installs to /opt/homebrew/bin by default (already on PATH for GUI apps and
# non-interactive shells). Override with:  PREFIX=/usr/local/bin ./build.sh
set -euo pipefail
cd "$(dirname "$0")"

PREFIX="${PREFIX:-/opt/homebrew/bin}"

if ! command -v cargo >/dev/null 2>&1; then
  # Pick up a rustup install that isn't on the current PATH.
  [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust: https://rustup.rs" >&2
  exit 1
fi

echo "building release binary..."
cargo build --release

mkdir -p "$PREFIX"
cp target/release/gptbrowser "$PREFIX/gptbrowser"
echo "installed: $PREFIX/gptbrowser"
"$PREFIX/gptbrowser" --help | head -3 || true
