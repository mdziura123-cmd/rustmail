# Lista testów ręcznych (Bazzite KDE)

Kod RustMail nie był uruchamiany przez autora na prawdziwym koncie, więc ta lista jest właściwym odbiorem aplikacji.
Przejdź ją po kolei na AppImage z GitHub Actions. Przy każdym punkcie zapisz: działa / nie działa / co zobaczyłeś.
Nie wklejaj do czatu haseł, tokenów ani treści prywatnych maili – wystarczy opis i zrzut ekranu z zasłoniętymi danymi.

## 0. Uruchomienie
1. `chmod +x RustMail-x86_64.AppImage` i uruchom dwuklikiem lub z terminala (`./RustMail-x86_64.AppImage`).
2. Okno się otwiera, nie jest białe. (Jeśli jest białe: `WEBKIT_DISABLE_DMABUF_RENDERER=0` lub `GDK_BACKEND=x11`.)
3. Zamknięcie i ponowne uruchomienie zachowuje logowanie (token jest w pęku kluczy KDE Wallet).

## 1. Konto i odczyt
- [ ] Logowanie przez Google (przeglądarka otwiera się, aplikacja dostaje zgodę).
- [ ] Foldery: Odebrane, Nieprzeczytane, Wysłane, Wersje robocze, Spam, Kosz, Archiwum, Gwiazdka, Ważne.
- [ ] Otwarcie wątku pokazuje dymki, polskie znaki są poprawne, wątek oznacza się jako przeczytany.
- [ ] „Wczytaj więcej” dokłada kolejne wątki.

## 2. Pisanie i wysyłanie
- [ ] Nowa wiadomość do samego siebie: wysyła się, dochodzi, ma poprawny podpis.
- [ ] Odpowiedz / Odpowiedz wszystkim / Przekaż dalej (w odpowiedzi nie ma cytowania całej korespondencji).
- [ ] Załącznik do 20 MB wysyła się; większy jest odrzucany z komunikatem.
- [ ] Czcionka i rozmiar w edytorze działają, wklejanie jest czystym tekstem.
- [ ] Wersja robocza: zapis, ponowne otwarcie z folderu „Wersje robocze”, usunięcie.

## 3. Bezpieczeństwo
- [ ] Obrazy z internetu są zablokowane do kliknięcia „Pokaż obrazy”.
- [ ] Kliknięcie linku pokazuje pasek potwierdzenia z adresem docelowym.
- [ ] Załącznik pobiera się dopiero po kliknięciu do ~/Downloads, nie nadpisuje istniejącego pliku.
- [ ] Plik o ryzykownym rozszerzeniu (np. test.sh) wymaga dodatkowego potwierdzenia.
- [ ] Wiadomość testowa „phishingowa” (np. od siebie z innego adresu, temat „Pilne: zweryfikuj konto”) dostaje ostrzeżenie; folder „Podejrzane” ją pokazuje; „Uznaj za bezpieczne” działa i można cofnąć.
- [ ] Listy nadawców: dodanie i usunięcie adresu/domeny; zablokowany nadawca dostaje czerwone ostrzeżenie.
- [ ] ClamAV (jeśli zainstalowany): plik testowy EICAR (tekst ze strony eicar.org) NIE zostaje zapisany. Bez ClamAV aplikacja pisze, że plik nie był skanowany.

## 4. Wyszukiwanie i tryb offline
- [ ] Szukaj: `from:ty@gmail.com`, `has:attachment`, zwykłe słowo. „Wyczyść” wraca do folderu.
- [ ] Ustawienia → Tryb offline → włącz (po potwierdzeniu). Otwórz kilka wątków.
- [ ] Odłącz sieć (tryb samolotowy / wyłącz Wi‑Fi): lista i otwarte wcześniej wątki są widoczne z żółtym banerem; wyszukiwanie działa lokalnie.
- [ ] Wątek, którego nie otwierałeś, pokazuje komunikat, że nie jest zapisany offline.
- [ ] Po powrocie sieci baner znika, a w ciągu minuty zmiany z Gmaila (np. nowa wiadomość z telefonu) pojawiają się na liście.
- [ ] „Wyłącz i usuń dane” czyści zapis (liczba zapisanych wątków = 0).
- [ ] Plik bazy ma uprawnienia tylko dla Ciebie: `ls -l ~/.local/share/pl.rustmail.client/cache/` → `-rw-------`.

## 5. Liczniki, powiadomienia, etykiety
- [ ] Odznaka z liczbą nieprzeczytanych przy „Odebrane” zgadza się z Gmailem (po minucie od zmiany).
- [ ] Ustawienia → powiadomienia włączone → „Wyślij próbne powiadomienie” pokazuje bańkę KDE. (Wymaga `notify-send`; na Bazzite zwykle jest.)
- [ ] Wyślij sobie wiadomość z drugiego konta: w ciągu ~minuty pojawia się powiadomienie (domyślnie bez nadawcy i tematu).
- [ ] Po włączeniu „Pokazuj nadawcę i temat” bańka pokazuje nadawcę i temat; dla wiadomości z ostrzeżeniem – nigdy.
- [ ] „+ Nowa etykieta” tworzy etykietę (widać ją też w Gmailu w przeglądarce); duplikat daje czytelny błąd.
- [ ] W wątku „Etykiety” → zaznacz/odznacz: zmiana widoczna w Gmailu; kliknięcie etykiety w panelu pokazuje jej wątki.

## 5b. Kolejka wysyłania
- [ ] Odłącz sieć, napisz wiadomość do siebie, kliknij „Wyślij”: pojawia się błąd „Brak połączenia” i propozycja zapisu w kolejce z ostrzeżeniem o braku szyfrowania.
- [ ] „Zapisz w kolejce do wysłania”: okno się zamyka, na dole widać „Do wysłania (1)”.
- [ ] Przywróć sieć: w ciągu ~minuty (lub „Wyślij teraz”) wiadomość wychodzi i jest w „Wysłane”, kolejka się opróżnia. Sprawdź, że przyszła tylko JEDNA kopia.
- [ ] Wiadomość z załącznikiem w kolejce wychodzi z załącznikiem.
- [ ] „Usuń” w oknie kolejki usuwa wiadomość bez wysyłania.
- [ ] Zamknij aplikację z wiadomością w kolejce, uruchom ponownie z siecią: wiadomość wychodzi.

## 6. Aktualizacje
- [ ] „Sprawdź teraz” w stopce; po wydaniu nowej wersji: instalacja i „Przywróć poprzednią wersję”.

## Co zgłosić
Numer wersji (stopka), punkt z listy, co się stało, tekst komunikatu błędu. Dla problemów z oknem: uruchom z terminala i wklej ostatnie linie.
