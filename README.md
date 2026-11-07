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

## Stan projektu (wersja 0.4.0, etap 4)

Działa (do potwierdzenia na Twoim komputerze): logowanie przez Google, odczyt wątków, redagowanie i wysyłanie
(etap 3: odpowiedzi, przekazywanie, wersje robocze, podpis, czcionki, akcje na konwersacji) oraz – nowe w etapie 4:

- **Wiadomości HTML** pokazywane po oczyszczeniu: bez skryptów, ramek, formularzy i niebezpiecznych stylów. Zawsze można
  przełączyć na zwykły tekst.
- **Zdalne obrazy są zablokowane**, dopóki nie klikniesz „Pokaż obrazy” (lub „Zawsze ufaj nadawcy”). Obraz pobiera aplikacja,
  tylko przez https, tylko z publicznych adresów (nigdy z Twojej sieci lokalnej), do 5 MB, typy PNG/JPEG/GIF/WebP.
- **Linki** otwierają się w przeglądarce dopiero po potwierdzeniu, z pokazanym prawdziwym adresem i ostrzeżeniami
  (tekst linku udaje inną stronę, adres IP, „@” w adresie, znaki xn--, brak szyfrowania).
- **Załączniki** pobierane tylko na kliknięcie do ~/Downloads, bez prawa uruchamiania i bez nadpisywania plików.
  Pliki wykonywalne i skrypty (także „faktura.pdf.exe”) wymagają dodatkowego potwierdzenia.

Ograniczenia etapu 4: obrazy wbudowane w wiadomość (cid:) nie są wyświetlane; zaufani nadawcy są zapamiętywani lokalnie
(w pamięci aplikacji, nie w Gmailu); pobieranie obrazu nie przechodzi przez przekierowania poza trzy kolejne;
to jeszcze nie jest skaner wirusów ani wykrywanie phishingu na podstawie treści (etapy 5–6). Z etapu 3 pozostają:
lokalny podpis, brak podpowiedzi adresów, przekazywanie bez załączników oryginału, brak operacji grupowych.

## Niezweryfikowane

Kod Rust i interfejs nie były kompilowane w środowisku, w którym powstały (brak sieci). Moduły `compose.rs`, `signature.rs` i `safety.rs`, logika odpowiedzi (`replyLogic.ts`) i analiza linków (`linkCheck.ts`)
zostały uruchomione i przetestowane osobno, a czyszczenie HTML (`mailHtml.ts`) sprawdzone w prawdziwej przeglądarce Chromium na
złośliwych przykładach; pozostały kod
(komunikacja z Gmailem, polecenia Tauri, komponenty React) zweryfikowano tylko składniowo i typowo, więc pierwszy build
na GitHubie może wymagać drobnych poprawek. Wysyłanie i zapis wersji roboczych nie były testowane na prawdziwym koncie.
