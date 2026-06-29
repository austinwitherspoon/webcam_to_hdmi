#!/usr/bin/env bash
set -euo pipefail

SRC_DIR="${1:-/tmp/webcam_to_hdmi-src}"
OUT_BIN="${2:-/tmp/webcam_to_hdmi}"

if [ ! -d "$SRC_DIR" ]; then
  echo "[build] source directory not found: $SRC_DIR" >&2
  exit 1
fi

echo "[build] Installing native build dependencies..."
export DEBIAN_FRONTEND=noninteractive
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  curl \
  ca-certificates \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  libdrm-dev

if ! command -v cargo >/dev/null 2>&1; then
  echo "[build] Installing Rust toolchain (rustup)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi

# shellcheck disable=SC1091
source "$HOME/.cargo/env"

echo "[build] Building release binary in $SRC_DIR ..."
cd "$SRC_DIR"
cargo build --release

echo "[build] Exporting binary to $OUT_BIN"
cp target/release/webcam_to_hdmi "$OUT_BIN"
chmod +x "$OUT_BIN"

echo "[build] Build complete: $OUT_BIN"
