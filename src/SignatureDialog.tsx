import { useState } from "react";
import { api, errText } from "./api";

interface Props {
  /** Podpis domyślny (dla kont bez własnego). */
  initial: string;
  accounts: { id: string; label: string }[];
  /** Podpisy własne kont (klucz: „gmail” albo „imap:adres”). */
  values: Record<string, string>;
  onClose: () => void;
  /** `id` = null oznacza podpis domyślny; `text` = null oznacza powrót konta do podpisu domyślnego. */
  onSaved: (id: string | null, text: string | null) => void;
}

export function SignatureDialog({ initial, accounts, values, onClose, onSaved }: Props) {
  const [sel, setSel] = useState("");
  const [text, setText] = useState(initial);
  const [useDefault, setUseDefault] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pick = (id: string) => {
    setSel(id);
    setError(null);
    if (id === "") { setText(initial); setUseDefault(false); }
    else { setText(values[id] ?? ""); setUseDefault(values[id] === undefined); }
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      if (sel === "") {
        await api.setSignature(text);
        onSaved(null, text.trim());
      } else if (useDefault) {
        await api.setSignatureFor(sel, null);
        onSaved(sel, null);
      } else {
        await api.setSignatureFor(sel, text);
        onSaved(sel, text.trim());
      }
    } catch (e) {
      setError(errText(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Podpis wiadomości">
      <div className="modal">
        <h3>Podpis wiadomości</h3>
        <p className="hint">
          Podpis jest zapisany tylko na tym komputerze i dodawany do nowych wiadomości oraz odpowiedzi (zwykły tekst).
          Nie jest synchronizowany z podpisem ustawionym w Gmailu. Każde konto może mieć własny podpis; konta bez własnego używają domyślnego.
        </p>
        <label className="check sound-row">
          <span>Dla konta</span>
          <select value={sel} disabled={busy} onChange={(e) => pick(e.target.value)}>
            <option value="">Domyślny</option>
            {accounts.map((a) => <option key={a.id} value={a.id}>{a.label}</option>)}
          </select>
        </label>
        {sel !== "" && (
          <label className="check">
            <input type="checkbox" checked={useDefault} disabled={busy} onChange={(e) => setUseDefault(e.target.checked)} />
            Użyj podpisu domyślnego
          </label>
        )}
        <textarea value={text} maxLength={2000} rows={6} disabled={useDefault} onChange={(e) => setText(e.target.value)} aria-label="Treść podpisu" />
        {error && <p className="error" role="alert">{error}</p>}
        <div className="modal-actions">
          <button className="primary" disabled={busy} onClick={() => void save()}>{busy ? "Zapisywanie…" : "Zapisz"}</button>
          <button disabled={busy} onClick={onClose}>Anuluj</button>
        </div>
      </div>
    </div>
  );
}
