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

## Stan projektu (wersja 0.3.0, etap 3)

Działa (do potwierdzenia na Twoim komputerze): logowanie przez Google w przeglądarce, odczyt wątków i widok dymkowy,
samoaktualizacja z GitHuba oraz – nowe w etapie 3 – nowa wiadomość, odpowiedz, odpowiedz wszystkim, przekaż dalej
(tekst sformatowany, DW/UDW, załączniki do 20 MB), wersje robocze (zapis ręczny, automatyczny co 20 s i kopia lokalna
na wypadek awarii), podpis, oraz akcje na konwersacji: archiwizuj, spam, kosz, przeczytana/nieprzeczytana, gwiazdka,
z możliwością cofnięcia.

Ograniczenia etapu 3: podpis jest lokalny (nie synchronizuje się z Gmailem), brak podpowiedzi adresów (to wymaga
osobnego uprawnienia do kontaktów), przekazywanie nie dołącza załączników oryginału, brak rozmiaru czcionki i obrazów
w treści, brak trwałego usuwania z kosza, brak operacji grupowych. Nadal brak: bezpieczne renderowanie HTML,
skanowanie załączników, wyszukiwanie, tryb offline, powiadomienia (kolejne etapy).

## Niezweryfikowane

Kod Rust i interfejs nie były kompilowane w środowisku, w którym powstały (brak sieci). Moduły `compose.rs` i
`signature.rs` oraz logika odpowiedzi (`replyLogic.ts`) zostały uruchomione i przetestowane osobno; pozostały kod
(komunikacja z Gmailem, polecenia Tauri, komponenty React) zweryfikowano tylko składniowo i typowo, więc pierwszy build
na GitHubie może wymagać drobnych poprawek. Wysyłanie i zapis wersji roboczych nie były testowane na prawdziwym koncie.
