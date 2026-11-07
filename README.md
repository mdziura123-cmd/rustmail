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

## Stan projektu (wersja 0.5.0, etap 5)

Działa (do potwierdzenia na Twoim komputerze): logowanie przez Google, odczyt, redagowanie i wysyłanie (etap 3),
bezpieczne wyświetlanie HTML, blokada obrazów, potwierdzenia linków i pobieranie załączników (etap 4) oraz – nowe
w etapie 5 – **ocena ryzyka phishingu liczona wyłącznie na Twoim komputerze** (treść poczty nie jest nigdzie wysyłana):

- wyniki SPF/DKIM/DMARC odczytywane z nagłówka dodanego przez Gmail (nagłówki podstawione przez nadawcę są ignorowane),
- nazwa nadawcy udająca znaną markę lub zawierająca obcy adres e-mail, domeny łudząco podobne do znanych (paypa1.com,
  arnazon.com, allegro-pl.com), znaki międzynarodowe w domenie, inny adres „Reply-To”,
- presja czasu i prośby o hasła/kody/dane karty w temacie i treści,
- linki: tekst niezgodny z adresem, „@” w adresie, adres IP, skracacze, brak szyfrowania,
- załączniki: pliki wykonywalne i skrypty, strony HTML, archiwa.

Każda ocena (brak / uwaga / podejrzane) ma wypisane powody po polsku. Nad wiadomością pojawia się pasek ostrzeżenia, na liście
plakietka, a przy wysokim ryzyku linki i załączniki wymagają dodatkowego potwierdzenia, a obrazy się nie ładują nawet od
zaufanego nadawcy. Folder „Podejrzane” pokazuje wiadomości z ostrzeżeniem spośród ostatnich 100 wątków skrzynki odbiorczej
(to widok w aplikacji – nic nie jest przenoszone ani usuwane w Gmailu). „To fałszywy alarm” zapisuje decyzję lokalnie.

Ograniczenia etapu 5: to reguły, nie gwarancja – część oszustw przejdzie, a część zwykłych wiadomości dostanie ostrzeżenie.
Lista marek jest wbudowana i ograniczona (ok. 30). Ocena na liście (z nagłówków) jest wstępna; pełna – po otwarciu wątku.
Folder „Podejrzane” sprawdza tylko 100 ostatnich wątków. Brak skanera antywirusowego i listy blokowanych nadawców (etap 6).

## Niezweryfikowane

Kod Rust i interfejs nie były kompilowane w środowisku, w którym powstały (brak sieci). Moduły `compose.rs`, `signature.rs`, `safety.rs`, `phishing.rs` i `dismissed.rs`, logika odpowiedzi (`replyLogic.ts`) i analiza linków (`linkCheck.ts`)
zostały uruchomione i przetestowane osobno, a czyszczenie HTML (`mailHtml.ts`) sprawdzone w prawdziwej przeglądarce Chromium na
złośliwych przykładach; pozostały kod
(komunikacja z Gmailem, polecenia Tauri, komponenty React) zweryfikowano tylko składniowo i typowo, więc pierwszy build
na GitHubie może wymagać drobnych poprawek. Wysyłanie i zapis wersji roboczych nie były testowane na prawdziwym koncie.
