import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, errText } from "./api";
import { MessageBody } from "./MessageBody";
import { badPass, isKeyAttachment, needsPass, pgpKind, pgpMsg, sigText } from "./pgp";
import type { Attachment, MessageView, PgpAtt, PgpImport, PgpOpened, PgpSig, SavedFile } from "./types";
import { fmtSize } from "./util";

interface Props {
  m: MessageView;
  /** Załączniki wiadomości (w IMAP podawane osobno). */
  atts?: Attachment[];
  strict: boolean;
  /** Pobiera źródło i odszyfrowuje / sprawdza podpis po stronie programu. */
  open: (passphrase: string | null) => Promise<PgpOpened>;
  /** Import klucza z załącznika (gdy wiadomość go zawiera). */
  importKey?: (a: Attachment) => Promise<PgpImport>;
  /** Zwykła treść i załączniki – pokazywane, dopóki wiadomość nie zostanie odszyfrowana. */
  children: ReactNode;
}

function SigChip({ s }: { s: PgpSig }) {
  const t = sigText(s);
  return (
    <span className={"chip " + t.cls} title={t.detail}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3l2 2h3v3l2 2-2 2v3h-3l-2 2-2-2H7v-3l-2-2 2-2V5h3zM9.5 10.5l2 2 3.5-3.5" /></svg>
      {t.label}
    </span>
  );
}

function importNote(r: PgpImport): string {
  const n = r.fingerprints.length;
  return r.newKeys > 0 || r.secret > 0
    ? `Zaimportowano klucz (${n}). Od teraz możesz szyfrować wiadomości do tej osoby i sprawdzać jej podpisy.`
    : "Ten klucz już był w Twoim pęku kluczy (zaktualizowano go, jeśli był nowszy).";
}

/** Pasek i treść wiadomości OpenPGP. Dla zwykłych wiadomości pokazuje tylko import klucza z załącznika. */
export function PgpMessage({ m, atts, strict, open, importKey, children }: Props) {
  const list = atts ?? m.attachments;
  const kind = pgpKind(m, list);
  const [res, setRes] = useState<PgpOpened | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [ask, setAsk] = useState(false);
  const [pass, setPass] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const live = useRef(true);
  useEffect(() => () => { live.current = false; }, []);

  const run = async (p: string | null, quiet: boolean) => {
    setBusy(true);
    if (!quiet) setErr(null);
    try {
      const r = await open(p);
      if (!live.current) return;
      setRes(r);
      setAsk(false);
      setPass("");
      setErr(null);
    } catch (e) {
      if (!live.current) return;
      const msg = errText(e);
      if (needsPass(msg)) {
        setAsk(true);
        if (!quiet) setErr(null);
      } else if (badPass(msg)) {
        setAsk(true);
        setErr(pgpMsg(msg));
      } else if (!quiet || kind === "signed") {
        setErr(pgpMsg(msg));
      }
    } finally {
      if (live.current) setBusy(false);
    }
  };

  // Podpis sprawdzamy od razu. Zaszyfrowaną próbujemy otworzyć bez pytania – uda się, gdy hasło pamięta gpg-agent.
  useEffect(() => {
    if (kind) void run(null, true);
    // Raz dla danej wiadomości.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [m.id, kind]);

  const keyAtts = importKey ? list.filter((a) => a.id && isKeyAttachment(a)) : [];
  const keyRow = keyAtts.length > 0 && (
    <div className="pgp-bar">
      {keyAtts.map((a, i) => (
        <button key={a.name + i} className="small" onClick={() => {
          setNote(null);
          importKey?.(a).then((r) => setNote(importNote(r))).catch((e) => setNote(errText(e)));
        }}>Importuj klucz OpenPGP „{a.name}”</button>
      ))}
      {note && <span className="hint" role="status">{note}</span>}
    </div>
  );

  if (!kind) return <>{children}{keyRow}</>;

  const decrypted = res?.decrypted ? res : null;
  const view: MessageView | null = decrypted
    ? { ...m, body: decrypted.text, html: decrypted.html, htmlOnly: !decrypted.text && !!decrypted.html, attachments: [] }
    : null;

  return (
    <>
      <div className={"pgp-bar" + (kind === "encrypted" ? " enc" : "")} role="group" aria-label="OpenPGP">
        {kind === "encrypted" && (
          <span className={"chip " + (decrypted ? "ok" : "new")} title="Treść i załączniki tej wiadomości są zaszyfrowane OpenPGP. Temat i adresy są jawne.">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4" /></svg>
            {decrypted ? "Odszyfrowano (OpenPGP)" : "Wiadomość zaszyfrowana"}
          </span>
        )}
        {res?.signature && <SigChip s={res.signature} />}
        {kind === "signed" && !res && busy && <span className="hint">Sprawdzanie podpisu…</span>}
        {kind === "encrypted" && !decrypted && !ask && (
          <button className="small primary" disabled={busy} onClick={() => void run(null, false)}>{busy ? "Odszyfrowywanie…" : "Odszyfruj"}</button>
        )}
        {ask && (
          <form className="pgp-pass" onSubmit={(e) => { e.preventDefault(); if (pass) void run(pass, false); }}>
            <input type="password" autoComplete="off" autoFocus placeholder="Hasło klucza OpenPGP" aria-label="Hasło klucza OpenPGP"
              value={pass} disabled={busy} onChange={(e) => setPass(e.target.value)} />
            <button className="small primary" disabled={busy || !pass}>{busy ? "Odszyfrowywanie…" : kind === "signed" ? "Sprawdź" : "Odszyfruj"}</button>
            <button type="button" className="small" disabled={busy} onClick={() => { setAsk(false); setPass(""); setErr(null); }}>Anuluj</button>
          </form>
        )}
        {res?.signature && <span className="hint pgp-detail">{sigText(res.signature).detail}</span>}
        {err && <span className="error" role="alert">{err}</span>}
      </div>
      {view ? (
        <>
          <MessageBody m={view} strict={strict} />
          {decrypted && decrypted.attachments.length > 0 && <DecryptedAtts list={decrypted.attachments} />}
        </>
      ) : children}
      {keyRow}
    </>
  );
}

function DecryptedAtts({ list }: { list: PgpAtt[] }) {
  return (
    <div className="attachments" aria-label="Odszyfrowane załączniki">
      {list.map((a, i) => <DecRow key={a.name + i} a={a} />)}
    </div>
  );
}

function DecRow({ a }: { a: PgpAtt }) {
  const [busy, setBusy] = useState(false);
  const [r, setR] = useState<SavedFile | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const key = isKeyAttachment(a);
  return (
    <div className="attach">
      <span>{a.name} · {fmtSize(a.size)}</span>
      <button className="small" disabled={busy} onClick={() => {
        setBusy(true);
        setNote(null);
        api.pgpSaveAttachment(a.name, a.dataBase64).then(setR).catch((e) => setNote(errText(e))).finally(() => setBusy(false));
      }}>{busy ? "Zapisywanie…" : "Zapisz"}</button>
      {key && (
        <button className="small" onClick={() => {
          setNote(null);
          api.pgpImport(null, a.dataBase64).then((res) => setNote(importNote(res))).catch((e) => setNote(errText(e)));
        }}>Importuj klucz</button>
      )}
      {r && (r.path ? <span className="ok-line">Zapisano: {r.path}</span> : <span className="warn-line">Plik nie został zapisany ({r.scanDetail}).</span>)}
      {r?.typeWarning && <span className="warn-line">⚠ {r.typeWarning}</span>}
      {note && <span className="hint">{note}</span>}
    </div>
  );
}
