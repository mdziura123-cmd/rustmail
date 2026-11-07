# RustMail – etap 2

Klient Gmail dla Linuksa: Tauri 2 + Rust + React.

## Co działa w tym etapie

- Układ trzech paneli, motyw jasny/ciemny/systemowy, regulowana szerokość paneli.
- Logowanie przez Google (OAuth 2.0 + PKCE, przekierowanie na 127.0.0.1), tokeny w KDE Wallet / GNOME Keyring.
- Foldery Gmail (Odebrane, Nieprzeczytane, Wysłane, Wersje robocze, Spam, Kosz, Archiwum, Gwiazdka, Ważne) – tylko odczyt.
- Konwersacje Gmail (wątki) w widoku dymkowym, rozwijanie i zwijanie, strony po 20 wątków.
- Treść pokazywana wyłącznie jako zwykły tekst (HTML jest zamieniany na tekst). Bezpieczne renderowanie HTML: etap 5.
- Samoaktualizacja z wydań GitHub (opis niżej).

## Czego jeszcze nie ma

Wysyłanie i redagowanie, oznaczanie i archiwizacja, lokalna baza i tryb offline, synchronizacja przyrostowa,
filtry bezpieczeństwa, załączniki (widać tylko nazwy), powiadomienia, folder „Podejrzane” (jest pusty).

## Konfiguracja Gmaila

Zobacz `docs/GOOGLE_OAUTH.md`.

## Budowanie

GitHub Actions buduje plik po każdej zmianie w gałęzi `main` (zakładka **Actions → artefakt**).
Alternatywnie lokalnie: `./scripts/build-appimage.sh` (distrobox, nie zmienia systemu hosta).

## Uruchomienie

```bash
chmod +x RustMail-x86_64.AppImage
./RustMail-x86_64.AppImage
# bez FUSE:
./RustMail-x86_64.AppImage --appimage-extract-and-run
```

Jeśli okno jest puste: `GDK_BACKEND=x11 ./RustMail-x86_64.AppImage`.

## Aktualizacje automatyczne

Aplikacja co 6 godzin (i 8 sekund po starcie) pyta GitHub o najnowsze wydanie. Gdy jest nowsze, pokazuje baner
z pytaniem „Zaktualizować?”. Dopiero po kliknięciu **Zaktualizuj teraz** pobiera plik, sprawdza sumę SHA-256
z wydania, zachowuje poprzednią wersję (`…AppImage.previous`) i podmienia plik. Potem proponuje ponowne uruchomienie.
W pasku stanu można wyłączyć automatyczne sprawdzanie, sprawdzić ręcznie albo przywrócić poprzednią wersję.

Wymagania:
- repozytorium musi być **publiczne** (prywatne wymagałoby tokenu w aplikacji),
- nowa wersja to **wydanie** (Releases) z tagiem `vX.Y.Z`; workflow sam ustawia wersję z tagu i dołącza
  `RustMail-x86_64.AppImage` oraz `RustMail-x86_64.AppImage.sha256`,
- aktualizacja działa tylko po uruchomieniu z pliku AppImage (nie z rozpakowanego katalogu),
- katalog z plikiem AppImage musi być zapisywalny.

Jak wydać nową wersję: GitHub → **Releases → Create a new release** → pole tagu: wpisz np. `v0.2.1` i **Create new tag**
→ **Publish release**. Workflow zbuduje aplikację i dołączy pliki (kilkanaście minut).

Ograniczenie: suma SHA-256 pochodzi z tego samego wydania, więc chroni przed uszkodzonym pobraniem, ale nie przed
przejęciem konta GitHub. Podpisy kryptograficzne wydań można dodać później.

## Niezweryfikowane

Kod nie był kompilowany ani uruchamiany w środowisku, w którym powstał (brak sieci), więc pierwszy build może wymagać poprawek.
Logowanie, pobieranie poczty i samoaktualizacja nie były testowane na prawdziwym koncie ani na Bazzite.
