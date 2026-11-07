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

## Stan projektu (wersja 0.7.0, etap 7)

Działa (do potwierdzenia na Twoim komputerze): logowanie przez Google, odczyt, redagowanie i wysyłanie (etap 3), bezpieczny HTML,
linki i załączniki (etap 4), ocena ryzyka phishingu z folderem „Podejrzane” (etap 5), skanowanie załączników i listy nadawców (etap 6) oraz – nowe w etapie 7:

- **Wyszukiwanie** (pole nad listą): zapytanie idzie do Gmaila, więc działa składnia Gmaila (`from:`, `has:attachment`, `after:2026/01/01`…).
  Bez sieci, przy włączonym trybie offline, przeszukuje zapisane lokalnie wątki (wszystkie słowa muszą wystąpić).
- **Tryb offline (opcjonalny, domyślnie wyłączony)** – przycisk „Tryb offline” na dole. Zapisuje na tym komputerze kopie przeglądanych
  wątków w bazie SQLite, żeby czytać i przeszukiwać je bez sieci. **Baza nie jest szyfrowana** (chronią ją tylko uprawnienia plików
  i ewentualne szyfrowanie dysku); nie zawiera tokenów ani haseł, załączników ani obrazów z internetu. Wyłączenie, wyczyszczenie,
  wylogowanie i zmiana konta usuwają zapisane wątki. Pamięć trzyma najwyżej 2000 wątków, w tym 200 z pełną treścią.
- **Synchronizacja przyrostowa** przez Gmail History API co minutę: odświeżane są tylko zmienione, zapisane wątki. Gdy Gmail uzna
  historię za zbyt starą albo zmian jest bardzo dużo, pamięć jest czyszczona i budowana od nowa.

Ograniczenia etapu 7: bez sieci nie wyślesz wiadomości ani nie wykonasz akcji (nie ma kolejki wysyłania). Ocena ryzyka w pamięci
offline pochodzi z chwili zapisu i może nie uwzględniać późniejszych zmian list nadawców. Wyszukiwanie offline obejmuje tylko to,
co zostało wcześniej otwarte lub wyświetlone na liście. Poprzednie funkcje (etap 6):

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
ostrożności: antywirus nie wykrywa każdego złośliwego pliku. Archiwa z hasłem nie są skanowane. Brak: powiadomienia, liczniki nieprzeczytanych, własne etykiety (kolejny etap).

## Niezweryfikowane

Etap 7: `store.rs` (rusqlite), synchronizacja History API, wyszukiwanie i polecenia Tauri nie były kompilowane. Zapytania SQL sprawdzono osobno na prawdziwym SQLite (schemat, filtry folderów, upsert, przycinanie), ale połączenie Rust–SQLite i wywołania Gmail History API zostaną sprawdzone dopiero przez CI i na Twoim koncie.

Kod Rust i interfejs nie były kompilowane w środowisku, w którym powstały (brak sieci). Moduły `offline.rs` (logika), a także `compose.rs`, `signature.rs`, `safety.rs`, `phishing.rs`, `dismissed.rs`, `filetype.rs`, `scan.rs` i `blocklist.rs`, logika odpowiedzi (`replyLogic.ts`) i analiza linków (`linkCheck.ts`)
zostały uruchomione i przetestowane osobno, a czyszczenie HTML (`mailHtml.ts`) sprawdzone w prawdziwej przeglądarce Chromium na
złośliwych przykładach; pozostały kod
(komunikacja z Gmailem, polecenia Tauri, komponenty React) zweryfikowano tylko składniowo i typowo, więc pierwszy build
na GitHubie może wymagać drobnych poprawek. Wysyłanie i zapis wersji roboczych nie były testowane na prawdziwym koncie.
