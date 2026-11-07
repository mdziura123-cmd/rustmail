import { useState } from "react";
import type { AuthStatus } from "./types";

interface Props {
  auth: AuthStatus;
  error: string | null;
  busy: boolean;
  onSave: (clientId: string, clientSecret: string) => void;
  onSignIn: () => void;
  onClear: () => void;
}

export function AuthPanel({ auth, error, busy, onSave, onSignIn, onClear }: Props) {
  const [id, setId] = useState("");
  const [secret, setSecret] = useState("");
  return (
    <div className="auth">
      <h2>Połącz konto Gmail</h2>
      {error && <p className="error" role="alert">{error}</p>}
      {!auth.configured ? (
        <>
          <p>
            RustMail łączy się z Gmailem przez Twój własny projekt Google Cloud. Instrukcję krok po kroku
            znajdziesz w pliku <code>docs/GOOGLE_OAUTH.md</code>. Wklej tutaj dane klienta typu „Aplikacja komputerowa”.
          </p>
          <label>
            Identyfikator klienta
            <input value={id} onChange={(e) => setId(e.target.value)} placeholder="….apps.googleusercontent.com" spellCheck={false} />
          </label>
          <label>
            Klucz tajny klienta
            <input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} spellCheck={false} />
          </label>
          <button className="solid" disabled={!id.trim() || !secret.trim()} onClick={() => onSave(id, secret)}>
            Zapisz dane klienta
          </button>
          <p className="hint">Dane zapisujemy w systemowym magazynie haseł (KDE Wallet / GNOME Keyring), nigdy w pliku.</p>
        </>
      ) : (
        <>
          <p>Dane klienta są zapisane. Logowanie odbywa się w przeglądarce, RustMail nie widzi Twojego hasła.</p>
          <button className="solid" disabled={busy} onClick={onSignIn}>
            {busy ? "Czekam na logowanie w przeglądarce…" : "Zaloguj przez Google"}
          </button>
          <button disabled={busy} onClick={onClear}>Zmień dane klienta</button>
        </>
      )}
    </div>
  );
}
