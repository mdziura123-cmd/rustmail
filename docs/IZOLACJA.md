# Wersja izolowana (piaskownica)

Instalacja: `bash install-sandbox.sh` (pobiera najnowsze wydanie, sprawdza sumę SHA-256, tworzy skrót „RustMail (izolowany)”).

## Co widzi aplikacja w piaskownicy
- tylko własne dane: `~/.local/share|.config|.cache/pl.rustmail.app`, folder aplikacji `~/.local/share/rustmail-sandbox` i `~/Downloads`;
- systemowe programy i biblioteki tylko do odczytu (`/usr`, `/etc`);
- Wayland (a gdy go brak – X11);
- internet (potrzebny do Gmaila);
- D-Bus przez filtr: wyłącznie pęk kluczy, powiadomienia i portal otwierania linków. Linki otwiera zwykła przeglądarka przez portal;
- nie widzi: reszty katalogu domowego, kluczy SSH, dokumentów, innych procesów.

## Diagnostyka
- `rustmail-sandbox --check` – czy system ma wszystko, czego trzeba;
- `rustmail-sandbox --print` – pokazuje pełne polecenie piaskownicy;
- `RUSTMAIL_SAFE_GPU=1 rustmail-sandbox` – wyłącza przyspieszanie DMABUF, jeśli okno jest puste.

## Uwagi
- Aktualizacje w aplikacji działają: plik aplikacji leży w katalogu zapisywalnym dla piaskownicy.
- Zwykła (nieizolowana) wersja nadal działa niezależnie.
- Nie testowano na prawdziwym Bazzite przed pierwszym uruchomieniem przez użytkownika.
