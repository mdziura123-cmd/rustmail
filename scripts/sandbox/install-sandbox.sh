#!/usr/bin/env bash
# Instaluje RustMail w wersji izolowanej (piaskownica bubblewrap).
#
# Użycie:
#   bash install-sandbox.sh                          # pobiera najnowsze wydanie z GitHuba
#   bash install-sandbox.sh /ścieżka/RustMail.AppImage   # używa pliku, który już masz
set -euo pipefail

REPO="mdziura123-cmd/rustmail"
ASSET="RustMail-x86_64.AppImage"
APP_DIR="$HOME/.local/share/rustmail-sandbox"
BIN_DIR="$HOME/.local/bin"
APPS_DIR="$HOME/.local/share/applications"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAW="https://raw.githubusercontent.com/$REPO/main/scripts/sandbox"

die() { echo "BŁĄD: $*" >&2; exit 1; }
for c in bwrap xdg-dbus-proxy gdbus curl sha256sum; do
  command -v "$c" >/dev/null 2>&1 || die "brakuje programu '$c'."
done

mkdir -p "$APP_DIR/shim" "$BIN_DIR" "$APPS_DIR"

# Pliki pomocnicze: z tego samego folderu, a jeśli ich tu nie ma – z GitHuba.
fetch_helper() {
  local name="$1" dest="$2"
  if [ -f "$HERE/$name" ]; then cp "$HERE/$name" "$dest"; else curl -fsSL "$RAW/$name" -o "$dest"; fi
  chmod +x "$dest"
}
fetch_helper rustmail-sandbox "$BIN_DIR/rustmail-sandbox"
fetch_helper xdg-open "$APP_DIR/shim/xdg-open"

# Aplikacja
if [ "${1:-}" != "" ]; then
  [ -f "$1" ] || die "nie ma pliku: $1"
  cp "$1" "$APP_DIR/$ASSET.new"
else
  echo "Pobieram najnowsze wydanie…"
  rel="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")" || die "nie udało się połączyć z GitHubem."
  url="$(printf '%s' "$rel" | grep -o '"browser_download_url": *"[^"]*'"$ASSET"'"' | head -n1 | cut -d'"' -f4)"
  sha_url="$(printf '%s' "$rel" | grep -o '"browser_download_url": *"[^"]*'"$ASSET"'.sha256"' | head -n1 | cut -d'"' -f4)"
  [ -n "$url" ] && [ -n "$sha_url" ] || die "w najnowszym wydaniu nie ma pliku $ASSET lub jego sumy."
  curl -fL --retry 3 -o "$APP_DIR/$ASSET.new" "$url"
  curl -fsSL -o "$APP_DIR/$ASSET.sha256" "$sha_url"
  expected="$(cut -d' ' -f1 "$APP_DIR/$ASSET.sha256")"
  actual="$(sha256sum "$APP_DIR/$ASSET.new" | cut -d' ' -f1)"
  [ "$expected" = "$actual" ] || { rm -f "$APP_DIR/$ASSET.new"; die "suma kontrolna się nie zgadza, nie instaluję."; }
  echo "Suma kontrolna poprawna."
fi
chmod +x "$APP_DIR/$ASSET.new"
mv -f "$APP_DIR/$ASSET.new" "$APP_DIR/$ASSET"

# Skrót w menu aplikacji
cat > "$APPS_DIR/rustmail-sandbox.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=RustMail (izolowany)
Comment=Klient Gmail w piaskownicy
Exec=$BIN_DIR/rustmail-sandbox
Icon=mail-message
Categories=Network;Email;
Terminal=false
DESKTOP
chmod +x "$APPS_DIR/rustmail-sandbox.desktop"

echo
"$BIN_DIR/rustmail-sandbox" --check || true
echo
echo "Gotowe. Uruchom z menu („RustMail (izolowany)”) albo poleceniem: $BIN_DIR/rustmail-sandbox"
echo "Dane (konto, baza, ustawienia) są te same co w zwykłej wersji, więc nie trzeba logować się od nowa."
