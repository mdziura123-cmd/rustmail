import { useState } from "react";
import { api, errText } from "./api";
import { riskyFile } from "./linkCheck";
import type { Attachment, MessageView } from "./types";
import { fmtSize } from "./util";

function Row({ m, a }: { m: MessageView; a: Attachment }) {
  const risky = riskyFile(a.name);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const download = async () => {
    if (!a.id) return;
    setBusy(true);
    setError(null);
    setConfirm(false);
    try {
      const saved = await api.downloadAttachment(m.id, a.id, a.name);
      setResult(saved.path);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={"attach" + (risky ? " risky" : "")}>
      <span>{a.name} · {fmtSize(a.size)}</span>
      {risky && <span className="badge-danger">możliwie niebezpieczny typ pliku</span>}
      {a.id ? (
        confirm ? (
          <>
            <span className="warn-line">Ten plik może uruchomić szkodliwy kod. Pobierz tylko, jeśli ufasz nadawcy. Zostanie zapisany bez prawa uruchamiania.</span>
            <button className="small solid" disabled={busy} onClick={() => void download()}>Pobierz mimo to</button>
            <button className="small" onClick={() => setConfirm(false)}>Anuluj</button>
          </>
        ) : (
          <button className="small" disabled={busy} onClick={() => (risky ? setConfirm(true) : void download())}>
            {busy ? "Pobieranie…" : "Pobierz"}
          </button>
        )
      ) : (
        <span className="note">brak możliwości pobrania</span>
      )}
      {result && <span className="ok-line">Zapisano: {result}</span>}
      {error && <span className="error">{error}</span>}
    </div>
  );
}

export function Attachments({ m }: { m: MessageView }) {
  if (m.attachments.length === 0) return null;
  return (
    <div className="attachments" aria-label="Załączniki">
      {m.attachments.map((a, i) => <Row key={a.name + i} m={m} a={a} />)}
    </div>
  );
}
