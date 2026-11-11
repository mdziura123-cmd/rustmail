import { useState } from "react";
import { loadTemplates, MAX_NAME, MAX_TEMPLATES, MAX_TEXT, saveTemplates, type Template } from "./templates";

export function Templates() {
  const [list, setList] = useState<Template[]>(loadTemplates);
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [err, setErr] = useState<string | null>(null);

  const commit = (next: Template[]) => { setList(next); saveTemplates(next); };
  const add = () => {
    const n = name.trim();
    if (!n || !text.trim()) { setErr("Podaj nazwę i treść szablonu."); return; }
    if (list.length >= MAX_TEMPLATES) { setErr(`Możesz mieć najwyżej ${MAX_TEMPLATES} szablonów.`); return; }
    if (list.some((t) => t.name.toLowerCase() === n.toLowerCase())) { setErr("Szablon o takiej nazwie już istnieje."); return; }
    setErr(null);
    commit([...list, { name: n, text: text.trim() }]);
    setName("");
    setText("");
  };

  return (
    <div className="rule-box">
      <h4>Szablony odpowiedzi</h4>
      <p className="hint">Gotowe teksty do wstawiania w nowej wiadomości (przycisk „Szablony” przy pisaniu). Są zapisywane na tym komputerze i w kopii zapasowej.</p>
      <ul className="addr-list">
        {list.map((t, i) => (
          <li key={t.name}>
            <span title={t.text}>{t.name} — {t.text.length > 60 ? t.text.slice(0, 60) + "…" : t.text}</span>
            <button className="small" onClick={() => commit(list.filter((_, j) => j !== i))}>Usuń</button>
          </li>
        ))}
        {list.length === 0 && <li className="note">Brak szablonów.</li>}
      </ul>
      <div className="modal-actions wrap">
        <input value={name} maxLength={MAX_NAME} placeholder="Nazwa, np. Dziękuję" aria-label="Nazwa szablonu" onChange={(e) => setName(e.target.value)} />
        <textarea value={text} maxLength={MAX_TEXT} rows={3} placeholder="Treść szablonu" aria-label="Treść szablonu" onChange={(e) => setText(e.target.value)} />
        <button className="small" onClick={add}>Dodaj szablon</button>
      </div>
      {err && <p className="error">{err}</p>}
    </div>
  );
}
