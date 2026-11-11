import { useCallback, useEffect, useState } from "react";
import { api, errText } from "./api";

interface Rule { id: number; field: string; needle: string; action: string }

const FIELD: Record<string, string> = { from: "Nadawca zawiera", subject: "Temat zawiera" };
const ACTION: Record<string, string> = {
  read: "oznacz jako przeczytaną", archive: "archiwizuj",
  flag1: "nadaj flagę: bardzo ważne", flag2: "nadaj flagę: ważne", flag3: "nadaj flagę: mniej ważne",
};

export function Rules() {
  const [rules, setRules] = useState<Rule[]>([]);
  const [field, setField] = useState("from");
  const [needle, setNeedle] = useState("");
  const [action, setAction] = useState("archive");
  const [err, setErr] = useState<string | null>(null);

  const load = useCallback(() => { api.rulesList().then(setRules).catch(() => setRules([])); }, []);
  useEffect(load, [load]);

  const add = async () => {
    setErr(null);
    try { await api.rulesAdd(field, needle, action); setNeedle(""); load(); } catch (e) { setErr(errText(e)); }
  };

  return (
    <div className="rule-box">
      <h4>Reguły automatyczne</h4>
      <p className="hint">
        Działają na nowych wiadomościach, które przyjdą, gdy aplikacja jest uruchomiona (także w tle). Starych wiadomości nie ruszają.
        Wiadomość przeczytana lub zarchiwizowana przez regułę nie wywołuje powiadomienia.
      </p>
      <ul className="addr-list">
        {rules.map((r) => (
          <li key={r.id}>
            <span>{FIELD[r.field] ?? r.field} „{r.needle}” → {ACTION[r.action] ?? r.action}</span>
            <button className="small" onClick={() => void api.rulesDelete(r.id).then(load)}>Usuń</button>
          </li>
        ))}
        {rules.length === 0 && <li className="hint">Brak reguł.</li>}
      </ul>
      <div className="modal-actions wrap">
        <select aria-label="Pole reguły" value={field} onChange={(e) => setField(e.target.value)}>
          {Object.entries(FIELD).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        <input value={needle} maxLength={120} placeholder="np. @newsletter.pl" aria-label="Tekst do dopasowania"
          onChange={(e) => setNeedle(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") void add(); }} />
        <select aria-label="Akcja reguły" value={action} onChange={(e) => setAction(e.target.value)}>
          {Object.entries(ACTION).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
        </select>
        <button onClick={() => void add()}>Dodaj regułę</button>
      </div>
      {err && <p className="error" role="alert">{err}</p>}
    </div>
  );
}
