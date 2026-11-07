import { useCallback, useEffect, useState } from "react";
import { api, errText } from "./api";
import type { SenderLists as Lists } from "./types";

interface Props {
  /** Adres nadawcy otwartej wiadomości (gdy okno otwarto z konwersacji); null = samo zarządzanie listami. */
  email: string | null;
  onClose: () => void;
  /** Wywoływane po zmianie list; `moveToSpam` – użytkownik chce też przenieść konwersację do spamu. */
  onChanged: (moveToSpam: boolean) => void;
}

const kindLabel = { blocked: "Zablokowani nadawcy", allowed: "Zaufani nadawcy" } as const;

export function SenderLists({ email, onClose, onChanged }: Props) {
  const [lists, setLists] = useState<Lists | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [entry, setEntry] = useState("");
  const [kind, setKind] = useState<"blocked" | "allowed">("blocked");
  const [spam, setSpam] = useState(true);
  const [changed, setChanged] = useState(false);
  const domain = email ? email.split("@").pop() ?? "" : "";

  const reload = useCallback(async () => {
    try { setLists(await api.getSenderLists()); } catch (e) { setError(errText(e)); }
  }, []);
  useEffect(() => { void reload(); }, [reload]);

  const add = async (k: "blocked" | "allowed", value: string, andSpam = false) => {
    setBusy(true);
    setError(null);
    try {
      await api.addSenderRule(k, value);
      setChanged(true);
      await reload();
      if (andSpam) { onChanged(true); return; }
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = async (k: "blocked" | "allowed", value: string) => {
    setBusy(true);
    setError(null);
    try {
      await api.removeSenderRule(k, value);
      setChanged(true);
      await reload();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };
  const close = () => { if (changed) onChanged(false); else onClose(); };

  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Listy nadawców">
      <div className="modal wide-modal">
        <h3>Listy nadawców</h3>
        <p className="hint">
          Listy działają tylko w RustMail (zapisane na tym komputerze). Zablokowany nadawca dostaje w aplikacji czerwone
          ostrzeżenie – wiadomości nie są usuwane ani przenoszone automatycznie. Zaufanie do adresu działa tylko wtedy, gdy Gmail
          potwierdza, że wiadomość naprawdę pochodzi od tego nadawcy (DMARC), bo sam adres można podrobić.
        </p>

        {email && (
          <div className="sender-now">
            <strong>{email}</strong>
            <label className="check">
              <input type="checkbox" checked={spam} onChange={(e) => setSpam(e.target.checked)} />
              Przy blokowaniu przenieś tę konwersację do spamu w Gmailu (da się cofnąć)
            </label>
            <div className="sender-actions">
              <button className="small" disabled={busy} onClick={() => void add("blocked", email, spam)}>Zablokuj ten adres</button>
              {domain && <button className="small" disabled={busy} onClick={() => void add("blocked", domain, spam)}>Zablokuj całą domenę {domain}</button>}
              <button className="small" disabled={busy} onClick={() => void add("allowed", email)}>Zawsze ufaj temu adresowi</button>
            </div>
          </div>
        )}

        {(["blocked", "allowed"] as const).map((k) => (
          <div key={k} className="rule-list">
            <h4>{kindLabel[k]} ({lists ? lists[k].length : "…"})</h4>
            {lists && lists[k].length === 0 && <p className="hint">Brak wpisów.</p>}
            <ul>
              {lists?.[k].map((v) => (
                <li key={v}>
                  <span>{v}</span>
                  <button className="linklike plain" disabled={busy} onClick={() => void remove(k, v)}>Usuń</button>
                </li>
              ))}
            </ul>
          </div>
        ))}

        <div className="rule-add">
          <input value={entry} onChange={(e) => setEntry(e.target.value)} placeholder="adres@firma.pl lub firma.pl" aria-label="Nowy wpis" />
          <select value={kind} onChange={(e) => setKind(e.target.value as "blocked" | "allowed")} aria-label="Lista">
            <option value="blocked">Zablokowani</option>
            <option value="allowed">Zaufani</option>
          </select>
          <button className="small" disabled={busy || !entry.trim()} onClick={() => { void add(kind, entry).then(() => setEntry("")); }}>Dodaj</button>
        </div>

        {error && <p className="error" role="alert">{error}</p>}
        <div className="modal-actions">
          <button className="primary" onClick={close}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
