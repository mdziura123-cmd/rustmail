#!/usr/bin/env bash
# Uruchamiaj na hoście (Bazzite). Tworzy kontener distrobox (Ubuntu 22.04),
# uruchamia testy i buduje RustMail-x86_64.AppImage. Nie modyfikuje systemu hosta.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="rustmail-build"
OUT="$ROOT/dist-release"

command -v distrobox >/dev/null || { echo "Brak distrobox (na Bazzite jest domyślnie)."; exit 1; }

if ! distrobox list 2>/dev/null | grep -qw "$NAME"; then
  distrobox create --name "$NAME" --image docker.io/library/ubuntu:22.04 --yes
  distrobox enter "$NAME" -- bash "$ROOT/scripts/provision-container.sh"
fi

distrobox enter "$NAME" -- bash -lc "
  set -euo pipefail
  cd '$ROOT'
  export CARGO_HOME='$ROOT/.build-cache/cargo' RUSTUP_HOME='$ROOT/.build-cache/rustup'
  export PATH=\"\$CARGO_HOME/bin:\$PATH\"
  # W kontenerze nie ma FUSE – narzędzia AppImage mają się rozpakować same.
  export APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true
  npm install
  npm run typecheck
  (cd src-tauri && cargo test)
  npm run tauri build -- --bundles appimage
"

BUNDLE="$(ls -t "$ROOT"/src-tauri/target/release/bundle/appimage/*.AppImage | head -n1)"
mkdir -p "$OUT"
cp "$BUNDLE" "$OUT/RustMail-x86_64.AppImage"
chmod +x "$OUT/RustMail-x86_64.AppImage"
(cd "$OUT" && sha256sum RustMail-x86_64.AppImage > RustMail-x86_64.AppImage.sha256)
printf 'RustMail %s\nzbudowano: %s\n' "$(grep -m1 '"version"' "$ROOT/package.json" | tr -dc '0-9.')" "$(date -u +%FT%TZ)" > "$OUT/VERSION.txt"
echo "Gotowe: $OUT/RustMail-x86_64.AppImage"
