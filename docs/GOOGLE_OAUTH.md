# Konfiguracja logowania Google (OAuth) dla RustMail

RustMail nie ma wbudowanych danych klienta Google. Zakładasz własny, darmowy projekt (około 10 minut).
Nazwy w konsoli Google bywają zmieniane, więc jeśli jakiejś pozycji nie widzisz, szukaj podobnej.

1. Wejdź na https://console.cloud.google.com i zaloguj się kontem Google, które chcesz podłączyć.
2. Utwórz projekt (lista projektów u góry → **Nowy projekt**), np. „RustMail”.
3. **Interfejsy API i usługi → Biblioteka →** wyszukaj **Gmail API → Włącz**.
4. **Ekran zgody OAuth** (w nowszej konsoli: **Google Auth Platform**):
   - typ użytkowników: **Zewnętrzni**,
   - nazwa aplikacji: RustMail, e-mail pomocy i kontaktowy: Twój adres,
   - zakresy (Data Access): dodaj `https://www.googleapis.com/auth/gmail.readonly`,
   - użytkownicy testowi (Audience): dodaj swój adres Gmail.
5. **Dane logowania → Utwórz dane logowania → Identyfikator klienta OAuth →** typ aplikacji **Aplikacja komputerowa**.
   Skopiuj **Identyfikator klienta** i **Klucz tajny klienta**.
6. W RustMail wklej oba dane w panelu „Połącz konto Gmail”, kliknij **Zapisz dane klienta**, potem **Zaloguj przez Google**.
7. W przeglądarce pojawi się ostrzeżenie „Google nie zweryfikowało tej aplikacji”. To normalne dla własnego projektu:
   **Zaawansowane → Przejdź do RustMail (niebezpieczne)** i zezwól na dostęp do odczytu poczty.

## Ważne

- W trybie **testowym** Google unieważnia token po **7 dniach** i trzeba zalogować się ponownie. Aby tego uniknąć,
  w ekranie zgody kliknij **Opublikuj aplikację** (status „W produkcji”). Dla użytku osobistego weryfikacja nie jest potrzebna,
  zostanie tylko ostrzeżenie z kroku 7.
- Etap 2 prosi wyłącznie o zakres **tylko do odczytu**. Wysyłanie, archiwizacja i etykiety (etapy 3+) będą wymagały
  szerszego zakresu, więc wtedy trzeba będzie zalogować się ponownie.
- Klucz tajny klienta i token odświeżania trafiają do systemowego magazynu haseł (KDE Wallet / GNOME Keyring),
  nigdy do plików. Hasło do Google nigdy nie jest widoczne dla aplikacji.
- Odwołanie dostępu: przycisk **Wyloguj** w RustMail albo https://myaccount.google.com/permissions.
