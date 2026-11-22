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

## Stan projektu (wersja 0.13.0, etapy 10 i 12)

Działa (do potwierdzenia na Twoim komputerze): logowanie przez Google, odczyt, redagowanie i wysyłanie (etap 3), bezpieczny HTML,
linki i załączniki (etap 4), ocena ryzyka phishingu z folderem „Podejrzane” (etap 5), skanowanie załączników i listy nadawców (etap 6) oraz etap 7 (wyszukiwanie, tryb offline). Nowe w etapie 10: **wiele kont Gmail.** Przełącznik kont jest w panelu bocznym pod przyciskiem „Nowa wiadomość”: lista kont, „+ Dodaj konto Gmail” (otwiera przeglądarkę z wyborem konta Google) i „Wyloguj to konto”. Maksymalnie 5 kont. Tokeny wszystkich kont leżą w pęku kluczy systemu. Pracujesz na jednym koncie naraz: po przełączeniu lista, liczniki i powiadomienia dotyczą nowego konta, a zapisane wątki trybu offline są czyszczone i budują się od nowa (kolejka wysyłania zostaje i wysyła się dopiero po powrocie na konto, z którego powstała). Podpis, listy nadawców i ustawienia są wspólne dla wszystkich kont. Powiadomienia i liczniki dotyczą tylko aktywnego konta – nie sprawdzamy w tle pozostałych. Konto zalogowane przed tym etapem dopisuje się do listy automatycznie. Etap 12: **szyfrowanie lokalnej bazy** (SQLCipher, AES-256). Losowy klucz jest tworzony przy pierwszym uruchomieniu i trzymany w pęku kluczy systemu, tak jak tokeny; plik nazywa się `rustmail.enc.db`. Chroni to przed skopiowaniem samego pliku (np. z kopii zapasowej lub skradzionego dysku), ale nie przed programem uruchomionym na Twoim koncie, który może zapytać portfel o klucz. Jeśli klucz zniknie (np. wyczyszczony portfel), nieczytelny plik zostaje odłożony jako `rustmail.enc.db.unreadable` i powstaje nowa baza – kolejka wysyłania z tamtego pliku jest wtedy nieodzyskiwalna. Starą, niezaszyfrowaną bazę `rustmail.db` aplikacja przenosi przy pierwszym uruchomieniu (kolejka wysyłania i ustawienie trybu offline; zapisane wątki zbudują się od nowa), po czym nadpisuje ją zerami i usuwa (na dyskach SSD nie daje to gwarancji fizycznego wymazania). Nie zrobiono: powiadomień z kliknięciem i ikony w zasobniku. Etap 11: **skróty klawiszowe** jak w Gmailu (przycisk „Skróty” na dole albo klawisz `?`: j/k, x, e, #, !, r, a, f, c, s, Shift+I/U, /) oraz **zaznaczanie wielu konwersacji** polem przy każdym wątku z paskiem akcji hurtowych (archiwizuj, usuń/przywróć, spam, przeczytane/nieprzeczytane; do 50 naraz, z cofnięciem). Skróty są wyłączone podczas pisania w polu i przy otwartym oknie. Akcje hurtowe wykonują się po kolei, więc 50 wątków może potrwać kilka sekund. Etap 11 zmienia tylko interfejs (bez nowego kodu Rusta). Etap 9: **kolejka wysyłania.** Gdy przy wysyłaniu nie ma sieci, okno pisania proponuje „Zapisz w kolejce do wysłania”. Wiadomość (gotowy MIME, razem z załącznikami) czeka w lokalnej bazie **bez szyfrowania** i wychodzi sama po powrocie połączenia (sprawdzanie co minutę, tylko przy uruchomionej aplikacji). Przycisk „Do wysłania” na dole pokazuje kolejkę. Błąd inny niż brak sieci oznacza wiadomość jako nieudaną – nie jest ponawiana sama. Jeśli aplikacja zamknie się w trakcie wysyłania, wiadomość dostaje stan „nie wiadomo, czy wyszła” i czeka na Twoją decyzję. Rzadki przypadek graniczny: zerwanie sieci tuż po przyjęciu wiadomości przez Gmail może dać duplikat. Kolejka jest przypisana do konta i nie jest czyszczona przy wylogowaniu ani czyszczeniu pamięci offline. Limity: 50 wiadomości, 30 MB każda. Etap 8:

- **Liczniki nieprzeczytanych** przy „Odebrane”, „Spam” i własnych etykietach oraz w tytule okna (odświeżane co minutę).
- **Powiadomienia pulpitowe** (Ustawienia, domyślnie wyłączone) przez systemowe `notify-send`. Domyślnie bez treści; opcjonalnie
  nadawca i temat, nigdy dla wiadomości z ostrzeżeniem. Przy starcie nic nie jest ogłaszane. Nie ma powiadomień, gdy aplikacja jest zamknięta.
- **Własne etykiety Gmaila:** lista w panelu bocznym, tworzenie, przypisywanie i zdejmowanie w wątku. Etykiet systemowych nie da się
  tędy zmienić. Foldery etykiet wymagają sieci (nie są zapisywane offline).
- **Ustawienia** (przycisk na dole) i **lista testów ręcznych** w `docs/TESTY_BAZZITE.md`.

Etap 7:

- **Wyszukiwanie** (pole nad listą): zapytanie idzie do Gmaila, więc działa składnia Gmaila (`from:`, `has:attachment`, `after:2026/01/01`…).
  Bez sieci, przy włączonym trybie offline, przeszukuje zapisane lokalnie wątki (wszystkie słowa muszą wystąpić).
- **Tryb offline (opcjonalny, domyślnie wyłączony)** – przycisk „Tryb offline” na dole. Zapisuje na tym komputerze kopie przeglądanych
  wątków w bazie SQLite, żeby czytać i przeszukiwać je bez sieci. **Baza jest szyfrowana** (SQLCipher, klucz w pęku kluczy systemu; patrz etap 12); nie zawiera tokenów ani haseł, załączników ani obrazów z internetu. Wyłączenie, wyczyszczenie,
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

Etap 10: rejestr kont i przełączanie nie były uruchamiane (logika rejestru w `accounts.rs` jest testowana osobno). Etap 12: szyfrowanie SQLCipher i migracja starej bazy nie były kompilowane ani uruchamiane (SQLCipher nie jest dostępny w środowisku autora; logika pomocnicza w `dbkey.rs` jest testowana). Pierwsza kompilacja z `bundled-sqlcipher-vendored-openssl` zbuduje OpenSSL ze źródeł, więc będzie dłuższa. Etap 9: `store.rs` (kolejka), `flush_outbox` i interfejs kolejki nie były kompilowane (zapytania SQL sprawdzono na SQLite). Etap 8: `poll_mail`, `notify-send`, wywołania etykiet Gmaila i interfejs nie były kompilowane ani uruchamiane (logika `labels.rs` i `notify.rs` jest testowana osobno). Etap 7: `store.rs` (rusqlite), synchronizacja History API, wyszukiwanie i polecenia Tauri nie były kompilowane. Zapytania SQL sprawdzono osobno na prawdziwym SQLite (schemat, filtry folderów, upsert, przycinanie), ale połączenie Rust–SQLite i wywołania Gmail History API zostaną sprawdzone dopiero przez CI i na Twoim koncie.

Kod Rust i interfejs nie były kompilowane w środowisku, w którym powstały (brak sieci). Moduły `offline.rs`, `labels.rs`, `notify.rs` (logika), a także `compose.rs`, `signature.rs`, `safety.rs`, `phishing.rs`, `dismissed.rs`, `filetype.rs`, `scan.rs` i `blocklist.rs`, logika odpowiedzi (`replyLogic.ts`) i analiza linków (`linkCheck.ts`)
zostały uruchomione i przetestowane osobno, a czyszczenie HTML (`mailHtml.ts`) sprawdzone w prawdziwej przeglądarce Chromium na
złośliwych przykładach; pozostały kod
(komunikacja z Gmailem, polecenia Tauri, komponenty React) zweryfikowano tylko składniowo i typowo, więc pierwszy build
na GitHubie może wymagać drobnych poprawek. Wysyłanie i zapis wersji roboczych nie były testowane na prawdziwym koncie.

## Motywy kolorystyczne

Wybierasz je w lewym panelu, na dole („Motyw”): Systemowy, Jasny, Ciemny, Proton (fiolet i biel), Proton ciemny,
Błękit i biel, Solarized jasny, Solarized ciemny. Wszystkie kolory są jednolite – bez rozmycia i przezroczystości.
Kolory każdego motywu są w jednym miejscu: `src/themes.css` (zmienne `--bg`, `--accent` itd.). Nowy motyw = nowy blok
`:root[data-theme="nazwa"]` plus wpis w `src/themes.ts`.

## Emoji

Kolorowe emoji są wycinane z wyświetlanego tekstu (tematy, nadawcy, treść wiadomości, etykiety). Powód: silnik WebKit
w niektórych systemach (np. Bazzite) wysypuje się przy rysowaniu kolorowych czcionek COLRv1 (błąd biblioteki Skia),
co zawieszało foldery Spam i Nieprzeczytane. Wiadomości w Gmailu nie są zmieniane – to tylko sposób wyświetlania.

## Filtr spamu

Stopka → „Filtr spamu”. Cztery silniki (nagłówki, reguły, Bayes uczący się z Twoich decyzji, otwarte listy złośliwych domen) i cztery tryby: wyłączony / tylko oznaczaj / przenoś tylko pewne / przenoś wszystko. Domyślnie wyłączony. Wiadomości z gwiazdką, z Twoimi etykietami i z Twoim udziałem nie są ruszane; każde przeniesienie można cofnąć w oknie filtra. Listy pobierane są z `malware-filter.gitlab.io` i `phish.co.za` (raz na dobę).

## Import poczty

Stopka → „Import poczty”. Wczytuje pliki `.mbox` i `.eml` wyeksportowane z Outlooka, Yahoo, Thunderbirda itp. do Gmaila (z oryginalnymi datami; każdy plik → osobna etykieta, bez Odebranych i bez statusu nieprzeczytane). Nie wymaga haseł do innych kont. Kontakty importuje sam Google (contacts.google.com → Importuj).

## Poczta IMAP (WP, O2 i inne)

Stopka → „Poczta IMAP (WP)”. Osobne okno do czytania i porządkowania poczty z serwerów IMAP (szyfrowane połączenie, port 993). Hasło trafia do systemowego sejfu. Na razie bez wysyłania i bez pobierania załączników. W WP trzeba włączyć dostęp przez IMAP w ustawieniach poczty i użyć hasła do aplikacji zewnętrznych.

## Nowości z serii 1.9 (skrót)

- **Działanie w tle:** ikona w zasobniku z kropką przy nowych wiadomościach, jedna kopia aplikacji, stały skrót w menu (przypięcie do paska przetrwa aktualizacje), kliknięcie powiadomienia otwiera wiadomość.
- **Wysyłka:** cofnięcie wysyłki (5–30 s), zaplanowana wysyłka (do roku naprzód; wiadomość czeka w lokalnej bazie, wysyła się tylko przy uruchomionej aplikacji).
- **Odkładanie wiadomości:** Gmail (wraca do skrzynki jako nieprzeczytana) i WP (tylko lokalnie – aplikacja ukrywa wiadomość na liście, na serwerze nic się nie zmienia).
- **Reguły automatyczne:** nadawca/temat zawiera → przeczytana, archiwizuj, flaga. Działają na nowych wiadomościach, a przyciskiem „Sprawdź…/Zastosuj” także na starszych (Gmail ok. 200 wątków, WP 25 na konto).
- **Podpisy** osobne dla każdego konta, **szablony odpowiedzi** (do 15, w Ustawieniach → Poczta).
- **Podgląd obrazów** (PNG, JPEG, GIF do 8 MB, po skanie ClamAV) w załącznikach.
- **Wyszukiwanie w załącznikach:** Gmail robi to sam; dla WP `załącznik:fraza` przeszukuje pliki tekstowe w 25 najnowszych wiadomościach folderu.
- **Kalendarz lokalny** (ikona w dolnej belce): wydarzenia, powtarzanie (dzień/tydzień/miesiąc/rok), przypomnienia, „Dodaj do kalendarza” z wiadomości. Dane w zaszyfrowanej bazie, bez połączenia z pocztą.
- **Kopia zapasowa i synchronizacja:** plik JSON (kontakty, flagi, reguły, wydarzenia, ustawienia wyglądu, szablony) oraz synchronizacja przez wskazany folder (plik `rustmail-sync.json`, **niezaszyfrowany**, tylko dopisywanie – usunięcia nie są przenoszone).
- **Kopia bazy przy nowej wersji** (`dbsafe`) oraz dziennik zdarzeń w Ustawieniach.
- Czego nie ma: wspólnej skrzynki Gmail+WP, podglądu PDF (wymagałby osłabienia zabezpieczeń aplikacji), czytania PDF w wyszukiwaniu WP.

## Bezpieczeństwo poczty (seria 1.10)

- **Weryfikacja nadawcy:** przy wiadomościach Gmaila zielona tarcza „Nadawca zweryfikowany”, gdy Gmail potwierdził DMARC dla domeny z pola „Od”, a czerwona „Nadawca niezweryfikowany”, gdy DMARC się nie powiódł. Zweryfikowany nadawca nie oznacza bezpiecznej treści. W Poczcie WP tarcza opiera się na nagłówku `X-WP-DKIM-Status` od serwera WP: zielona, gdy podpis DKIM jest prawidłowy i należy do domeny z pola „Od”; brak tarczy, gdy podpisała inna domena (np. firma rozsyłająca newslettery) albo nagłówek występuje więcej niż raz (mógł go podstawić nadawca). Nagłówek jest brany pod uwagę tylko dla kont na serwerze wp.pl.
- **Nowy nadawca:** etykieta przy adresach spoza lokalnej książki adresowej (uzupełnianej z wysłanych i odebranych). Pokazuje się dopiero, gdy książka ma co najmniej 10 adresów.
- **Wypisz się:** gdy newsletter podaje nagłówek `List-Unsubscribe`. Po potwierdzeniu otwiera stronę wypisania (tylko https) w przeglądarce albo okno nowej wiadomości na adres wypisania. Nic nie dzieje się samo.
- **Źródło wiadomości:** nagłówki i pełna treść jako zwykły tekst (Gmail i WP), z kopiowaniem. Bardzo duże wiadomości są przycinane do 3 MB.
- **Wyciszanie rozmowy** (ikona dzwonka): brak powiadomień o kolejnych wiadomościach. Gmail wycisza wątek, WP rozmowę o tym samym temacie (bez Re:/Odp:/Fwd:). Wyciszenie leży w zaszyfrowanej bazie lokalnej.
- **Blokada kodem** (Ustawienia → Bezpieczeństwo): PIN lub hasło, blokada przy uruchomieniu i po bezczynności (1–60 min) albo przyciskiem „Zablokuj teraz”. W portfelu systemu leży tylko skrót PBKDF2-HMAC-SHA256 (210 000 iteracji, losowa sól). Po 5 błędnych próbach kolejne są wstrzymywane (30 s, potem coraz dłużej, najwyżej 15 min). W czasie blokady powiadomienia nie pokazują nadawcy ani tematu. Okno kalendarza blokuje się razem z oknem poczty (od 1.10.3), a przypomnienia w czasie blokady nie pokazują tytułu wydarzenia. Czas bezczynności liczy tylko okno poczty. To ochrona przed podejrzeniem poczty, a nie szyfrowanie: kto ma dostęp do Twojego konta w systemie, może odczytać portfel.
- **Ostrzeżenia o phishingu w Poczcie WP** (od 1.10.2): po otwarciu wiadomości te same reguły co w Gmailu (podrobione domeny i marki, fałszywe linki, groźne załączniki, presja czasu, prośby o hasło) oraz wynik podpisu z nagłówka WP. Bardzo podejrzana wiadomość otwiera się w trybie ścisłym (zablokowane linki i podglądy), z przyciskiem „fałszywy alarm”. Na liście wiadomości WP ostrzeżenia jeszcze nie widać – ocena wymaga pobrania treści.

## Szyfrowanie i podpis OpenPGP (seria 1.11)

- **Jak działa:** RustMail korzysta z systemowego GnuPG (`gpg`, w Bazzite jest domyślnie) – sam nie implementuje kryptografii. Ma **własny pęk kluczy** w `~/.local/share/pl.rustmail.app/gnupg` (prawa 0700), oddzielny od `~/.gnupg`, i własnego gpg-agenta. GnuPG nie łączy się z siecią: wyłączone jest pobieranie kluczy z serwerów kluczy i WKD.
- **Klucze** (Ustawienia → Szyfrowanie OpenPGP): utworzenie klucza dla konta (Ed25519/Curve25519, ważny 3 lata), import z wklejonego tekstu, z pliku albo jednym kliknięciem z załącznika `.asc` w wiadomości, kopiowanie i zapis klucza publicznego, kopia klucza prywatnego (wymaga hasła), usuwanie. Przy każdym kluczu widać odcisk do porównania z właścicielem.
- **Wysyłanie** (ikony w oknie nowej wiadomości): podpis, szyfrowanie i dołączenie własnego klucza publicznego. Format PGP/MIME (RFC 3156) – otwiera się w Thunderbirdzie, Proton Mail, Mailvelope i innych. Szyfrowanie działa tylko wtedy, gdy każdy odbiorca (Do, DW, UDW) ma klucz; inaczej wiadomość nie zostanie wysłana, a okno pokaże adresy bez klucza. Wiadomość zawsze jest szyfrowana też do własnego klucza (czytelna w Wysłanych). Odbiorcy UDW są ukryci – inni nie poznają ich kluczy. Temat i adresy pozostają jawne. Wiadomość szyfrowana nie jest zapisywana na serwerze jako (jawna) wersja robocza. Klucze bez AEAD/OCB, żeby wiadomości otwierały się także poza GnuPG 2.4.
- **Odbieranie:** przy wiadomości zaszyfrowanej (PGP/MIME albo blok w treści) przycisk „Odszyfruj”; odszyfrowana treść i załączniki są tylko w pamięci (nie trafiają do pamięci podręcznej). Podpisy są sprawdzane od razu: prawidłowy, nieprawidłowy (treść zmieniona), brak klucza nadawcy, klucz wygasły lub unieważniony, oraz ostrzeżenie, gdy podpis jest poprawny, ale klucz nie należy do adresu z pola „Od”.
- **Hasło klucza** podaje się w oknie programu; trafia do GnuPG przez standardowe wejście (nie w argumentach i nie w pliku). gpg-agent pamięta je 10 minut od użycia (najwyżej godzinę) i zapomina po zamknięciu programu albo po „Zapomnij zapamiętane hasła”.
- Czego nie ma: szyfrowania tematu (protected headers), Autocrypt, S/MIME, pobierania kluczy z serwerów kluczy.
