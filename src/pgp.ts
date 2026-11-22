// Pomocnicze funkcje OpenPGP po stronie interfejsu: rozpoznawanie wiadomości, kody błędów, ustawienia domyślne.
import type { Attachment, MessageView, PgpSig } from "./types";

/** Błąd „podaj hasło klucza” (gpg-agent go nie pamięta). */
export const needsPass = (msg: string) => msg.startsWith("PGP_HASLO:");
/** Błąd „złe hasło klucza”. */
export const badPass = (msg: string) => msg.startsWith("PGP_ZLE_HASLO:");
/** Komunikat bez wewnętrznego kodu. */
export const pgpMsg = (msg: string) => msg.replace(/^PGP_(ZLE_)?HASLO:\s*/, "");

export type PgpKind = "encrypted" | "signed" | null;

const ENC_NAME = /^(encrypted|msg|message)\.asc$/i;

/** Czy wiadomość wygląda na zaszyfrowaną albo podpisaną OpenPGP (decyduje i tak serwer po pobraniu źródła). */
export function pgpKind(m: MessageView, atts: Attachment[] = m.attachments): PgpKind {
  const text = (m.body || "") + "\n" + (m.html || "");
  if (atts.some((a) => a.mime === "application/pgp-encrypted" || ENC_NAME.test(a.name))) return "encrypted";
  if (/-----BEGIN PGP MESSAGE-----/.test(text)) return "encrypted";
  if (atts.some((a) => a.mime === "application/pgp-signature" || /^signature\.asc$/i.test(a.name))) return "signed";
  if (/-----BEGIN PGP SIGNED MESSAGE-----/.test(text)) return "signed";
  return null;
}

/** Załącznik z kluczem publicznym (do zaimportowania jednym kliknięciem). */
export function isKeyAttachment(a: { name: string; mime: string }): boolean {
  if (a.mime === "application/pgp-keys") return true;
  return /\.asc$/i.test(a.name) && /(openpgp|pgp|pubkey|public[-_ ]?key|klucz|0x[0-9a-f]{8,})/i.test(a.name) && !/^(signature|encrypted)\.asc$/i.test(a.name);
}

/** Odcisk w grupach po 4 znaki (łatwiej porównać przez telefon). */
export function fmtFpr(f: string): string {
  return f.replace(/(.{4})/g, "$1 ").trim();
}

export function sigText(s: PgpSig): { cls: "ok" | "bad" | "new"; label: string; detail: string } {
  const who = s.signer ? ` – ${s.signer}` : "";
  const fpr = s.fingerprint ? ` Odcisk klucza: ${fmtFpr(s.fingerprint)}.` : s.keyId ? ` Identyfikator klucza: ${s.keyId}.` : "";
  switch (s.status) {
    case "good":
      return s.fromMatches
        ? { cls: "ok", label: "Podpis OpenPGP prawidłowy", detail: `Treść nie została zmieniona od podpisania${who}.${fpr} Porównaj odcisk z nadawcą innym kanałem, jeśli to pierwsza wiadomość od niego.` }
        : { cls: "new", label: "Podpis prawidłowy, ale nie nadawcy", detail: `Podpis jest poprawny, ale klucz nie należy do adresu z pola „Od”${who}.${fpr}` };
    case "bad":
      return { cls: "bad", label: "Podpis OpenPGP NIEPRAWIDŁOWY", detail: "Treść zmieniono po podpisaniu albo podpis jest podrobiony. Nie ufaj tej wiadomości." };
    case "no_key":
      return { cls: "new", label: "Podpis OpenPGP – brak klucza", detail: `Nie masz klucza publicznego nadawcy, więc nie da się sprawdzić podpisu.${fpr} Zaimportuj klucz w Ustawieniach → Szyfrowanie OpenPGP.` };
    case "expired_key":
      return { cls: "new", label: "Podpis kluczem wygasłym", detail: `Klucz nadawcy stracił ważność${who}.${fpr}` };
    case "expired_sig":
      return { cls: "new", label: "Podpis wygasł", detail: `Podpis ma przekroczoną datę ważności${who}.${fpr}` };
    case "revoked_key":
      return { cls: "bad", label: "Podpis kluczem UNIEWAŻNIONYM", detail: `Właściciel unieważnił ten klucz – mógł zostać skradziony${who}.${fpr}` };
    default:
      return { cls: "bad", label: "Nie udało się sprawdzić podpisu", detail: fpr.trim() };
  }
}

// Ustawienia domyślne okna nowej wiadomości (lokalnie, bez treści).
const K_SIGN = "rm.pgp.sign";
const K_KEY = "rm.pgp.attachKey";
function getFlag(k: string): boolean {
  try { return localStorage.getItem(k) === "1"; } catch { return false; }
}
function setFlag(k: string, on: boolean) {
  try { localStorage.setItem(k, on ? "1" : "0"); } catch { /* pamięć niedostępna */ }
}
export const getSignDefault = () => getFlag(K_SIGN);
export const setSignDefault = (on: boolean) => setFlag(K_SIGN, on);
export const getAttachKeyDefault = () => getFlag(K_KEY);
export const setAttachKeyDefault = (on: boolean) => setFlag(K_KEY, on);
