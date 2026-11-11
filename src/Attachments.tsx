import { useState } from "react";
import { api, errText } from "./api";
import { riskyFile } from "./linkCheck";
import type { Attachment, MessageView, SavedFile } from "./types";
import { fmtSize } from "./util";

function ScanResult({ r }: { r: SavedFile }) {
  const scan =
    r.scan === "clean" ? "Sprawdzono antywirusem ClamAV: nie wykryto zagrożeń."
    : r.scan === "unavailable" ? "" // bez ClamAV nic nie pokazujemy – użytkownik nie korzysta ze skanowania
    : r.scan === "failed" ? `Skanowanie antywirusowe nie powiodło się (${r.scanDetail}). Plik nie został sprawdzony.`
    : "";
  return (
    <span className="scan-result">
      {r.scan === "infected" ? (
        <span className="warn-line">
          Zablokowano: antywirus wykrył zagrożenie ({r.scanDetail}). Plik NIE został zapisany na dysku.
        </span>
      ) : (
        <>
          <span className="ok-line">Zapisano: {r.path}</span>
          {scan && <span className={r.scan === "clean" ? "ok-line" : "warn-line"}>{scan}</span>}
        </>
      )}
      {r.typeWarning && <span className="warn-line">⚠ {r.typeWarning}</span>}
    </span>
  );
}

type Downloader = (a: Attachment) => Promise<SavedFile>;
type Previewer = (a: Attachment) => Promise<{ dataUrl: string }>;

const isImage = (name: string) => /\.(png|jpe?g|gif)$/i.test(name);

function Row({ m, a, strict, dl, pv }: { m: MessageView; a: Attachment; strict: boolean; dl?: Downloader; pv?: Previewer }) {
  const risky = riskyFile(a.name);
  const needsConfirm = risky || strict;
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<SavedFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [img, setImg] = useState<string | null>(null);

  const preview = async () => {
    if (!a.id) return;
    if (img) { setImg(null); return; }
    setBusy(true);
    setError(null);
    try {
      const r = await (pv ? pv(a) : api.previewAttachment(m.id, a.id));
      setImg(r.dataUrl);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const download = async () => {
    if (!a.id) return;
    setBusy(true);
    setError(null);
    setConfirm(false);
    try {
      setResult(await (dl ? dl(a) : api.downloadAttachment(m.id, a.id, a.name)));
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
            <span className="warn-line">{strict ? "Ta wiadomość została oceniona jako wysokiego ryzyka (phishing). " : ""}{risky ? "Ten plik może uruchomić szkodliwy kod. " : ""}Pobierz tylko, jeśli ufasz nadawcy. Plik zostanie zapisany bez prawa uruchamiania.</span>
            <button className="small solid" disabled={busy} onClick={() => void download()}>Pobierz mimo to</button>
            <button className="small" onClick={() => setConfirm(false)}>Anuluj</button>
          </>
        ) : (
          <button className="small" disabled={busy} onClick={() => (needsConfirm ? setConfirm(true) : void download())}>
            {busy ? "Pobieranie i skanowanie…" : "Pobierz"}
          </button>
        )
      ) : (
        <span className="note">brak możliwości pobrania</span>
      )}
      {a.id && !risky && !strict && isImage(a.name) && (
        <button className="small" disabled={busy} onClick={() => void preview()}>{img ? "Ukryj podgląd" : "Podgląd"}</button>
      )}
      {img && <img className="att-preview" src={img} alt={a.name} />}
      {result && <ScanResult r={result} />}
      {error && <span className="error">{error}</span>}
    </div>
  );
}

export function Attachments({ m, strict, dl, pv }: { m: MessageView; strict: boolean; dl?: Downloader; pv?: Previewer }) {
  if (m.attachments.length === 0) return null;
  return (
    <div className="attachments" aria-label="Załączniki">
      {m.attachments.map((a, i) => <Row key={a.name + i} m={m} a={a} strict={strict} dl={dl} pv={pv} />)}
    </div>
  );
}
