import { useState } from "react";
import { api, errText } from "./api";
import { ActIcon } from "./ActIcon";

interface Props {
  /** gmail = aktywne konto Gmail; imap = konto IMAP z `account`. */
  kind: "gmail" | "imap";
  account: string;
  /** Adres nadawcy (bez nazwy). */
  email: string;
  /** Po przeniesieniu: liczba przeniesionych wiadomości. */
  onDone: (moved: number) => void;
  onError: (text: string) => void;
}

/** „Spam i zablokuj nadawcę”: najpierw liczy wiadomości, pokazuje potwierdzenie, dopiero potem przenosi. */
export function SenderSpamButton({ kind, account, email, onDone, onError }: Props) {
  const [ask, setAsk] = useState<{ count: number; more: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  if (!email || !email.includes("@")) return null;

  const start = async () => {
    setBusy(true);
    try {
      if (kind === "gmail") setAsk({ count: await api.senderSpamCount(email), more: false });
      else { const r = await api.imapSenderSpam(account, email, false); setAsk({ count: r.found, more: r.more }); }
    } catch (e) { onError(errText(e)); } finally { setBusy(false); }
  };
  const apply = async () => {
    setBusy(true);
    try {
      const moved = kind === "gmail" ? await api.senderSpamApply(email) : (await api.imapSenderSpam(account, email, true)).moved;
      setAsk(null);
      onDone(moved);
    } catch (e) { onError(errText(e)); } finally { setBusy(false); }
  };
  const label = `Spam i zablokuj nadawcę ${email}`;
  return (
    <>
      <button title={label} aria-label={label} disabled={busy} onClick={() => void start()}><ActIcon n="ban" /></button>
      {ask && (
        <div className="modal-back" role="dialog" aria-modal="true" aria-label="Spam i zablokuj nadawcę"
          onKeyDown={(e) => { if (e.key === "Escape" && !busy) setAsk(null); }}>
          <div className="modal">
            <h3>Zablokować {email}?</h3>
            <p>
              {ask.count === 0
                ? "Poza Spamem i Koszem nie ma innych wiadomości od tego nadawcy."
                : `Do Spamu trafi ${ask.count} ${ask.count === 1 ? "wiadomość" : "wiadomości"} od tego nadawcy${kind === "imap" ? " ze wszystkich folderów (oprócz Wysłanych i Roboczych)" : ""}.`}
              {ask.more && " To więcej niż 500 – za jednym razem przeniesie się pierwsze 500, resztę przeniesiesz, powtarzając akcję."}
            </p>
            <p className="hint">
              Nadawca trafi też na listę zablokowanych: jego kolejne wiadomości będą od razu przenoszone do Spamu, bez
              powiadomień. Możesz to cofnąć w Opcjach nadawcy (ikona osoby) albo w Ustawieniach.
            </p>
            <div className="modal-actions">
              <button className="primary warn" disabled={busy} onClick={() => void apply()}>
                {busy ? "Przenoszenie…" : ask.count > 0 ? "Przenieś do Spamu i zablokuj" : "Zablokuj nadawcę"}
              </button>
              <button disabled={busy} onClick={() => setAsk(null)}>Anuluj</button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
