# Konfiguracja logowania Google (OAuth) dla RustMail

RustMail nie ma wbudowanych danych klienta Google. Zakładasz własny, darmowy projekt (około 10 minut).
Nazwy w konsoli Google bywają zmieniane, więc jeśli jakiejś pozycji nie widzisz, szukaj podobnej.

1. Wejdź na https://console.cloud.google.com i zaloguj się kontem Google, które chcesz podłączyć.
2. Utwórz projekt (lista projektów u góry → **Nowy projekt**), np. „RustMail”.
3. **Interfejsy API i usługi → Biblioteka →** wyszukaj **Gmail API → Włącz**.
4. **Ekran zgody OAuth** (w nowszej konsoli: **Google Auth Platform**):
   - typ użytkowników: **Zewnętrzni**,
   - nazwa aplikacji: RustMail, e-mail pomocy i kontaktowy: Twój adres,
   - zakresy (Data Access): dodaj `https://www.googleapis.com/auth/gmail.modify` (od wersji 0.3.0; wcześniejsze wersje używały `gmail.readonly`),
   - użytkownicy testowi (Audience): dodaj swój adres Gmail.
5. **Dane logowania → Utwórz dane logowania → Identyfikator klienta OAuth →** typ aplikacji **Aplikacja komputerowa**.
   Skopiuj **Identyfikator klienta** i **Klucz tajny klienta**.
6. W RustMail wklej oba dane w panelu „Połącz konto Gmail”, kliknij **Zapisz dane klienta**, potem **Zaloguj przez Google**
   (albo wbuduj je w aplikację, patrz sekcja na końcu tego pliku).
7. W przeglądarce pojawi się ostrzeżenie „Google nie zweryfikowało tej aplikacji”. To normalne dla własnego projektu:
   **Zaawansowane → Przejdź do RustMail (niebezpieczne)** i zezwól na dostęp do odczytu poczty.

## Ważne

- W trybie **testowym** Google unieważnia token po **7 dniach** i trzeba zalogować się ponownie. Aby tego uniknąć,
  w ekranie zgody kliknij **Opublikuj aplikację** (status „W produkcji”). Dla użytku osobistego weryfikacja nie jest potrzebna,
  zostanie tylko ostrzeżenie z kroku 7.
- Od wersji 0.3.0 (etap 3) RustMail prosi o zakres `gmail.modify`: czytanie, wysyłanie, wersje robocze, etykiety i kosz.
  Zakres **nie pozwala na trwałe usunięcie** wiadomości z Gmaila (usuwanie = przeniesienie do kosza).
  Po aktualizacji ze starszej wersji dodaj ten zakres w Data Access i kliknij w aplikacji „Zaloguj ponownie”.
- Klucz tajny klienta i token odświeżania trafiają do systemowego magazynu haseł (KDE Wallet / GNOME Keyring),
  nigdy do plików. Hasło do Google nigdy nie jest widoczne dla aplikacji.
- Odwołanie dostępu: przycisk **Wyloguj** w RustMail albo https://myaccount.google.com/permissions.

## Logowanie jak w Thunderbirdzie (dane klienta wbudowane w aplikację)

Po wykonaniu kroków 1–5 możesz wbudować dane klienta w aplikację, żeby w programie była tylko opcja
„Zaloguj przez Google”, bez formularza:

1. W repozytorium na GitHubie: **Settings → Secrets and variables → Actions → New repository secret**.
2. Dodaj dwa sekrety: `GOOGLE_CLIENT_ID` (identyfikator klienta) i `GOOGLE_CLIENT_SECRET` (klucz tajny klienta).
3. Uruchom budowanie ponownie (**Actions → Build AppImage → Run workflow**) i pobierz nowy plik.

Uwagi:
- Dane trafiają do pliku AppImage. Dla aplikacji komputerowych Google nie traktuje klucza tajnego klienta jako tajemnicy,
  ale każdy, kto pobierze Twój AppImage (np. z publicznego wydania), może go wyciągnąć i użyć Twojego projektu jako własnego.
  Dla użytku osobistego to akceptowalne.
- Tak działa też Thunderbird, ale Mozilla ma zarejestrowaną i zweryfikowaną przez Google aplikację. Własny projekt
  pokaże ostrzeżenie „Google nie zweryfikowało tej aplikacji” (krok 7) i w trybie testowym ma limit 100 użytkowników.
  Publiczna, zweryfikowana aplikacja z dostępem do poczty wymaga u Google przeglądu bezpieczeństwa (zwykle płatnego).
