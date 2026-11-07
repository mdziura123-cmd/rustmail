# RustMail

Klient Gmail dla Linuksa: Tauri 2 + Rust + React. Układ trzech paneli (foldery, lista, widok dymkowy),
motyw jasny/ciemny/systemowy, minimalne uprawnienia Tauri, integracja z Gmail API i skrypt budujący AppImage.

## Budowanie (Bazzite, bez zmian w systemie hosta)

```bash
./scripts/build-appimage.sh
```

Skrypt tworzy kontener distrobox z Ubuntu 22.04, instaluje w nim Rust, Node i WebKitGTK,
uruchamia `tsc` oraz `cargo test`, a potem buduje AppImage. Pierwszy build trwa kilkanaście minut.
Wynik: `dist-release/RustMail-x86_64.AppImage` (+ `.sha256`, `VERSION.txt`).

## Uruchomienie

```bash
chmod +x dist-release/RustMail-x86_64.AppImage
./dist-release/RustMail-x86_64.AppImage
# bez FUSE:
./dist-release/RustMail-x86_64.AppImage --appimage-extract-and-run
```

Jeśli okno jest puste, spróbuj `WEBKIT_DISABLE_DMABUF_RENDERER=0` lub `GDK_BACKEND=x11`.

## Stan projektu (wersja 0.6.0, etap 6)

Działa (do potwierdzenia na Twoim komputerze): logowanie przez Google, odczyt, redagowanie i wysyłanie (etap 3), bezpieczny HTML,
linki i załączniki (etap 4), ocena ryzyka phishingu z folderem „Podejrzane” (etap 5) oraz – nowe w etapie 6:

- **Skanowanie załączników lokalnym ClamAV** (`clamdscan`, a gdy go brak – `clamscan`), jeśli jest zainstalowany. Plik z wykrytym
  zagrożeniem **nie jest zapisywany** w ~/Downloads. Gdy ClamAV nie ma albo skan się nie udał, plik jest zapisywany, ale aplikacja
  wyraźnie pisze, że **nie został sprawdzony antywirusem**. Nic nie jest wysyłane do internetu.
- **Kontrola typu pliku po zawartości:** ostrzeżenie, gdy „faktura.pdf” w środku jest programem, skryptem lub stroną HTML
  (także gdy obraz/PDF ma niezgodną zawartość).
- **Listy nadawców** (przycisk „Nadawca…” w konwersacji oraz „Listy nadawców” na dole): zablokowany adres lub domena dostaje
  czerwone ostrzeżenie w aplikacji, opcjonalnie możesz przenieść konwersację do spamu w Gmailu (z cofnięciem). Zaufany nadawca nie
  dostaje ostrzeżeń od nadawcy/domeny, **ale tylko gdy Gmail potwierdza DMARC=pass** (adres można podrobić); ostrzeżenia
  o załącznikach i linkach nadal działają.

Ograniczenia etapu 6: listy działają tylko w RustMail – Gmail API nie pozwala blokować nadawców bez dodatkowego, szerszego
uprawnienia, więc aplikacja niczego nie ukrywa i nie usuwa automatycznie. Skaner działa tylko, jeśli ClamAV jest zainstalowany
w systemie (na Bazzite np. warstwą rpm-ostree albo w kontenerze – aplikacja szuka programów w PATH hosta). Skan nie zastępuje
ostrożności: antywirus nie wykrywa każdego złośliwego pliku. Archiwa z hasłem nie są skanowane. Brak: wyszukiwanie, tryb offline,
powiadomienia (kolejne etapy).

## Niezweryfikowane

Kod Rust i interfejs nie były kompilowane w środowisku, w którym powstały (brak sieci). Moduły `compose.rs`, `signature.rs`, `safety.rs`, `phishing.rs`, `dismissed.rs`, `filetype.rs`, `scan.rs` i `blocklist.rs`, logika odpowiedzi (`replyLogic.ts`) i analiza linków (`linkCheck.ts`)
zostały uruchomione i przetestowane osobno, a czyszczenie HTML (`mailHtml.ts`) sprawdzone w prawdziwej przeglądarce Chromium na
złośliwych przykładach; pozostały kod
(komunikacja z Gmailem, polecenia Tauri, komponenty React) zweryfikowano tylko składniowo i typowo, więc pierwszy build
na GitHubie może wymagać drobnych poprawek. Wysyłanie i zapis wersji roboczych nie były testowane na prawdziwym koncie.
