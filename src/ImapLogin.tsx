import { useState } from "react";
import { api, errText } from "./api";
import type { ImapAccount } from "./types";

interface Props {
  onClose: () => void;
  onAdded: (a: ImapAccount) => void;
}

export function ImapLogin({ onClose, onAdded }: Props) {
  const [email, setEmail] = useState("");
  const [pass, setPass] = useState("");
  const [host, setHost] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const login = async () => {
    setBusy(true); setErr(null);
    try { onAdded(await api.imapLogin(email, pass, host)); }
    catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Dodaj konto IMAP">
      <form className="modal" onSubmit={(e) => { e.preventDefault(); if (!busy && email.trim() && pass) void login(); }}>
        <h3>Dodaj konto pocztowe (WP, O2 i inne IMAP)</h3>
        <p className="hint">
          Połączenie jest szyfrowane, a hasło trafia do systemowego sejfu (KDE Wallet). W WP włącz w ustawieniach poczty
          dostęp przez IMAP i użyj hasła do aplikacji zewnętrznych, jeśli WP je wymaga. Na razie można czytać i porządkować pocztę
          (bez wysyłania).
        </p>
        <div className="imap-login">
          <label>Adres e-mail<input value={email} onChange={(e) => setEmail(e.target.value)} placeholder="ty@wp.pl" autoComplete="off" spellCheck={false} /></label>
          <label>Hasło<input type="password" value={pass} onChange={(e) => setPass(e.target.value)} autoComplete="off" /></label>
          <label>Serwer IMAP (puste dla wp.pl i o2.pl)<input value={host} onChange={(e) => setHost(e.target.value)} placeholder="imap.wp.pl" spellCheck={false} /></label>
        </div>
        {err && <p className="error" role="alert">{err}</p>}
        <div className="modal-actions">
          <button className="primary" type="submit" disabled={busy || !email.trim() || !pass}>{busy ? "Łączę…" : "Dodaj konto"}</button>
          <button type="button" disabled={busy} onClick={onClose}>Anuluj</button>
        </div>
      </form>
    </div>
  );
}
