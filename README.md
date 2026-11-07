# RustMail – etap 1 (szkielet)

Klient Gmail dla Linuksa: Tauri 2 + Rust + React. Ten etap zawiera układ trzech paneli
(foldery, lista, widok dymkowy), motyw jasny/ciemny/systemowy, zmianę szerokości paneli,
minimalne uprawnienia Tauri i skrypt budujący AppImage. **Nie ma jeszcze integracji z Gmailem.**

## Budowanie przez GitHub Actions (zalecane, bez instalacji czegokolwiek lokalnie)

1. Załóż darmowe konto na github.com i utwórz **prywatne** repozytorium `rustmail`.
2. W katalogu projektu:
   ```bash
   git init -b main && git add . && git commit -m "RustMail etap 1"
   git remote add origin https://github.com/TWOJ_LOGIN/rustmail.git
   git push -u origin main
   ```
3. Wejdź w zakładkę **Actions**. Workflow „Build AppImage” startuje sam po wysłaniu kodu (można go też uruchomić ręcznie).
4. Po zakończeniu pobierz artefakt `RustMail-x86_64.AppImage` (to archiwum zip z plikiem AppImage i sumą SHA-256).
5. Rozpakuj, nadaj prawa wykonywania (`chmod +x RustMail-x86_64.AppImage`) i uruchom. Archiwa z GitHuba nie zachowują bitu wykonywania.

Wydanie z plikiem do pobrania: `git tag v0.1.0 && git push origin v0.1.0`.

## Budowanie lokalnie (alternatywa)

### Lokalnie w distrobox (Bazzite, bez zmian w systemie hosta)

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

## Stan etapu 1

Działa: układ okna, wybór folderów, zmiana szerokości paneli (zapamiętywana), motyw, rozwijanie
i zwijanie dymków, gwiazdka (lokalnie), komenda `app_info` z testami jednostkowymi.
Nieaktywne (celowo wyłączone z opisem w podpowiedzi): „Dodaj konto Gmail” (etap 2) i „Nowa wiadomość” (etap 3).
Przycisk „Pokaż układ z danymi przykładowymi” w pasku stanu wyświetla fikcyjne dane wyłącznie do oceny wyglądu.

## Niezweryfikowane

Kod nie był kompilowany w środowisku, w którym powstał (brak sieci). Pierwszy build może wymagać drobnych poprawek.
Działanie AppImage na Bazzite KDE trzeba potwierdzić na Twoim komputerze.
