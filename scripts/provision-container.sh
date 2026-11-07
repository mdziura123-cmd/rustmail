#!/usr/bin/env bash
# Uruchamiany WEWNĄTRZ kontenera Ubuntu 22.04 (jednorazowo).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

sudo apt-get update
sudo apt-get install -y build-essential curl wget file git pkg-config \
  libwebkit2gtk-4.1-dev libssl-dev libxdo-dev libayatana-appindicator3-dev \
  librsvg2-dev libgtk-3-dev patchelf desktop-file-utils zsync

if ! command -v node >/dev/null || [ "$(node -p 'process.versions.node.split(".")[0]')" -lt 20 ]; then
  curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
  sudo apt-get install -y nodejs
fi

# Rust trzymamy w katalogu projektu, żeby nie ruszać ~/.cargo hosta.
export CARGO_HOME="$ROOT/.build-cache/cargo" RUSTUP_HOME="$ROOT/.build-cache/rustup"
if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
echo "Kontener gotowy."
