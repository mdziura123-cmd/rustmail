import { useState } from "react";
import { api, errText } from "./api";

interface Props {
  initial: string;
  onClose: () => void;
  onSaved: (text: string) => void;
}

export function SignatureDialog({ initial, onClose, onSaved }: Props) {
  const [text, setText] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.setSignature(text);
      onSaved(text.trim());
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
          Nie jest synchronizowany z podpisem ustawionym w Gmailu.
        </p>
        <textarea value={text} maxLength={2000} rows={6} onChange={(e) => setText(e.target.value)} aria-label="Treść podpisu" />
        {error && <p className="error" role="alert">{error}</p>}
        <div className="modal-actions">
          <button className="primary" disabled={busy} onClick={() => void save()}>{busy ? "Zapisywanie…" : "Zapisz"}</button>
          <button disabled={busy} onClick={onClose}>Anuluj</button>
        </div>
      </div>
    </div>
  );
}
