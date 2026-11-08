import { useRef, useState } from "react";
import { api, errText } from "./api";
import { defaultLabel, emlMessage, mboxMessages, toBase64 } from "./mailImport";

interface Props {
  onClose: () => void;
  onChanged: () => void;
}

interface Item { file: File; label: string }

const MAX_BYTES = 30 * 1024 * 1024;

export function ImportDialog({ onClose, onChanged }: Props) {
  const [items, setItems] = useState<Item[]>([]);
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(0);
  const [failed, setFailed] = useState(0);
  const [current, setCurrent] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const cancel = useRef(false);

  const pick = (list: FileList | null) => {
    if (!list) return;
    setItems(Array.from(list).map((file) => ({ file, label: defaultLabel(file.name) })));
    setMsg(null);
  };

  const start = async () => {
    cancel.current = false;
    setRunning(true); setDone(0); setFailed(0); setMsg(null);
    let ok = 0, bad = 0, streak = 0, lastErr = "";
    try {
      for (const it of items) {
        if (cancel.current) break;
        setCurrent(it.file.name);
        let labelId: string;
        try { labelId = await api.importLabel(it.label); }
        catch (e) { setMsg(`Etykieta „${it.label}”: ${errText(e)}`); bad += 1; setFailed(bad); continue; }
        const isEml = /\.eml$/i.test(it.file.name);
        const source: AsyncIterable<string> = isEml
          ? (async function* () { const m = await emlMessage(it.file); if (m) yield m; })()
          : mboxMessages(it.file);
        for await (const raw of source) {
          if (cancel.current) break;
          if (raw.length > MAX_BYTES) { bad += 1; setFailed(bad); continue; }
          try {
            await api.importMessage(toBase64(raw), labelId);
            ok += 1; streak = 0; setDone(ok);
          } catch (e) {
            // jedno powtórzenie na wypadek chwilowego ograniczenia po stronie Gmaila
            try {
              await new Promise((r) => setTimeout(r, 1500));
              await api.importMessage(toBase64(raw), labelId);
              ok += 1; streak = 0; setDone(ok);
            } catch (e2) {
              bad += 1; streak += 1; lastErr = errText(e2); setFailed(bad);
              if (streak >= 5) { setMsg("Przerwano po kolejnych błędach: " + lastErr); cancel.current = true; }
            }
          }
        }
      }
      setMsg((m) => m ?? (cancel.current ? "Przerwano." : `Gotowe. Zaimportowano: ${ok}${bad ? `, pominięto z błędem: ${bad}${lastErr ? ` (${lastErr})` : ""}` : ""}.`));
    } catch (e) {
      setMsg(errText(e));
    } finally {
      setRunning(false); setCurrent("");
      onChanged();
    }
  };

  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Import poczty">
      <div className="modal wide-modal">
        <h3>Import poczty z pliku</h3>
        <p className="hint">
          Wyeksportuj pocztę u starego dostawcy (Outlook, Yahoo, Thunderbird i inni oferują eksport do pliku .mbox lub .eml)
          i wskaż plik tutaj. Wiadomości trafią do Twojego Gmaila z oryginalnymi datami, każdy plik do osobnej etykiety
          (folder → etykieta). Nie trafią do Odebranych ani nie będą oznaczone jako nieprzeczytane. Żadne hasła do innych kont nie są potrzebne.
        </p>
        <input type="file" accept=".mbox,.mbx,.eml" multiple disabled={running} onChange={(e) => pick(e.target.files)} />
        {items.length > 0 && (
          <ul className="outbox-list">
            {items.map((it, i) => (
              <li key={it.file.name + i}>
                <span>{it.file.name} <span className="hint">({(it.file.size / 1048576).toFixed(1)} MB)</span></span>
                <label>Etykieta w Gmailu:{" "}
                  <input type="text" value={it.label} disabled={running} maxLength={100}
                    onChange={(e) => setItems((p) => p.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)))} />
                </label>
              </li>
            ))}
          </ul>
        )}
        {running && <p className="hint" role="status">Importuję {current}… zaimportowano {done}{failed ? `, błędów: ${failed}` : ""}. Nie zamykaj aplikacji.</p>}
        {msg && <p className="hint" role="status">{msg}</p>}
        <p className="hint">Kontakty: Gmail importuje je sam – wejdź na contacts.google.com → Importuj i wskaż plik .vcf lub .csv ze starego konta.</p>
        <div className="modal-actions">
          {running
            ? <button onClick={() => { cancel.current = true; }}>Przerwij</button>
            : <button className="primary" disabled={items.length === 0} onClick={() => void start()}>Importuj</button>}
          <button disabled={running} onClick={onClose}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
