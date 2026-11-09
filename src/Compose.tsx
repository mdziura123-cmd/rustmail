import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { RichEditor, type EditorApi } from "./RichEditor";
import { SpellBar } from "./SpellBar";
import type { ComposeInit } from "./replyLogic";
import { sanitizeHtml } from "./sanitize";
import type { OutAttachment, OutgoingMessage } from "./types";
import { fmtSize } from "./util";

export const BACKUP_KEY = "rm.compose.backup";
const MAX_ATTACH_BYTES = 20 * 1024 * 1024;
const AUTOSAVE_MS = 20000;

interface Props {
  init: ComposeInit;
  onSent: (info: { threadId: string; warning?: string | null }) => void;
  /** Konto nadawcy: "gmail" albo "imap:adres". */
  from?: string;
  senders?: { id: string; label: string }[];
  onQueued: () => void;
  onClose: (draftSaved: boolean) => void;
}

interface Att extends OutAttachment {
  size: number;
}

const MODE_TITLE: Record<ComposeInit["mode"], string> = {
  new: "Nowa wiadomość", reply: "Odpowiedź", replyAll: "Odpowiedź wszystkim", forward: "Przekazanie", draft: "Wersja robocza",
};

function readFile(file: File): Promise<Att> {
  return new Promise((resolve, reject) => {
    const fr = new FileReader();
    fr.onerror = () => reject(new Error(`Nie można odczytać pliku „${file.name}”.`));
    fr.onload = () => {
      const s = String(fr.result);
      resolve({ name: file.name, mime: file.type || "application/octet-stream", dataBase64: s.slice(s.indexOf(",") + 1), size: file.size });
    };
    fr.readAsDataURL(file);
  });
}

function backup(value: unknown | null) {
  try {
    if (value === null) localStorage.removeItem(BACKUP_KEY);
    else localStorage.setItem(BACKUP_KEY, JSON.stringify(value));
  } catch { /* brak dostępu do pamięci – kopia lokalna jest dodatkiem */ }
}

export function Compose({ init, onSent, onQueued, onClose, from: fromProp, senders }: Props) {
  const [from, setFrom] = useState(fromProp ?? "gmail");
  const imapAcc = from.startsWith("imap:") ? from.slice(5) : null;
  const [to, setTo] = useState(init.to);
  const [cc, setCc] = useState(init.cc);
  const [bcc, setBcc] = useState(init.bcc);
  const [showCc, setShowCc] = useState(Boolean(init.cc || init.bcc));
  const [subject, setSubject] = useState(init.subject);
  const [atts, setAtts] = useState<Att[]>([]);
  const [draftId, setDraftId] = useState<string | null>(init.draftId);
  const [status, setStatus] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [canQueue, setCanQueue] = useState(false);
  const [sending, setSending] = useState(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const html = useRef(init.html);
  const dirty = useRef(false);
  const saving = useRef(false);
  const draftRef = useRef<string | null>(init.draftId);
  const fileRef = useRef<HTMLInputElement>(null);
  const editorApi = useRef<EditorApi | null>(null);
  const [spellTick, setSpellTick] = useState(0);

  const build = useCallback((): OutgoingMessage => ({
    to, cc, bcc, subject,
    html: sanitizeHtml(html.current),
    inReplyTo: init.inReplyTo,
    references: init.references,
    threadId: init.threadId,
    draftId: draftRef.current,
    attachments: atts.map(({ name, mime, dataBase64 }) => ({ name, mime, dataBase64 })),
  }), [to, cc, bcc, subject, atts, init]);

  const markDirty = () => { dirty.current = true; };

  // Kopia lokalna na wypadek awarii (bez załączników).
  useEffect(() => {
    if (!dirty.current) return;
    const t = setTimeout(() => backup({ init: { ...init, draftId: draftRef.current }, to, cc, bcc, subject, html: html.current }), 1000);
    return () => clearTimeout(t);
  }, [to, cc, bcc, subject, init]);

  const saveDraft = useCallback(async (manual: boolean): Promise<boolean> => {
    if (saving.current) return false;
    saving.current = true;
    if (manual) setError(null);
    try {
      let id: string | null;
      if (imapAcc) {
        const uid = await api.imapSaveDraft(imapAcc, build(), draftRef.current ? Number(draftRef.current) : null);
        id = uid > 0 ? String(uid) : null;
      } else {
        id = await api.saveDraft(build());
      }
      draftRef.current = id;
      setDraftId(id);
      dirty.current = false;
      setStatus("Zapisano wersję roboczą " + new Date().toLocaleTimeString("pl-PL", { hour: "2-digit", minute: "2-digit" }));
      return true;
    } catch (e) {
      const m = errText(e);
      if (manual) setError(m);
      else setStatus("Nie udało się zapisać wersji roboczej: " + m);
      return false;
    } finally {
      saving.current = false;
    }
  }, [build, imapAcc]);

  // Automatyczny zapis co kilkanaście sekund, tylko gdy coś się zmieniło.
  useEffect(() => {
    const t = setInterval(() => { if (dirty.current && !sending) void saveDraft(false); }, AUTOSAVE_MS);
    return () => clearInterval(t);
  }, [saveDraft, sending]);

  const send = async () => {
    setError(null);
    setSending(true);
    try {
      if (imapAcc) {
        const res = await api.imapSend(imapAcc, build());
        if (draftRef.current) api.imapDeleteDraft(imapAcc, Number(draftRef.current)).catch(() => {});
        dirty.current = false;
        backup(null);
        onSent({ threadId: "", warning: res.warning });
        return;
      }
      const info = await api.sendMessage(build());
      dirty.current = false;
      backup(null);
      onSent({ threadId: info.threadId || init.threadId || "" });
    } catch (e) {
      const m = errText(e);
      setError(m);
      setCanQueue(m.startsWith("Brak połączenia"));
      setSending(false);
    }
  };

  const queue = async () => {
    setError(null);
    setSending(true);
    try {
      await api.queueMessage(build());
      dirty.current = false;
      backup(null);
      onQueued();
    } catch (e) {
      setError(errText(e));
      setSending(false);
    }
  };

  const addFiles = async (files: FileList | null) => {
    if (!files) return;
    setError(null);
    try {
      const added: Att[] = [];
      let total = atts.reduce((s, a) => s + a.size, 0);
      for (const f of Array.from(files)) {
        total += f.size;
        if (total > MAX_ATTACH_BYTES) throw new Error("Załączniki nie mogą łącznie przekraczać 20 MB.");
        added.push(await readFile(f));
      }
      setAtts((a) => [...a, ...added]);
      markDirty();
    } catch (e) {
      setError(errText(e));
    }
    if (fileRef.current) fileRef.current.value = "";
  };

  const discard = async () => {
    setError(null);
    try {
      if (draftRef.current) {
        if (imapAcc) await api.imapDeleteDraft(imapAcc, Number(draftRef.current));
        else await api.deleteDraft(draftRef.current);
      }
      backup(null);
      onClose(false);
    } catch (e) {
      setError(errText(e));
    }
  };

  const requestClose = async () => {
    if (!dirty.current) { backup(null); onClose(Boolean(draftRef.current)); return; }
    setConfirmClose(true);
  };

  const saveAndClose = async () => {
    if (await saveDraft(true)) { backup(null); onClose(true); }
    else setConfirmClose(false);
  };

  return (
    <section className="compose" aria-label={MODE_TITLE[init.mode]}>
      <header className="compose-head">
        <h2>{MODE_TITLE[init.mode]}</h2>
        <span className="status-text" aria-live="polite">{status}</span>
        <span className="grow" />
        <button className="small" disabled={sending} onClick={() => void saveDraft(true)}>Zapisz wersję roboczą</button>
        <button className="small" disabled={sending} onClick={() => void requestClose()}>Zamknij</button>
      </header>

      {confirmClose && (
        <div className="confirm" role="alertdialog">
          <span>Wiadomość ma niezapisane zmiany.</span>
          <button className="small primary" onClick={() => void saveAndClose()}>Zapisz wersję roboczą</button>
          <button className="small" onClick={() => { backup(null); onClose(Boolean(draftRef.current)); }}>Odrzuć zmiany</button>
          <button className="small" onClick={() => setConfirmClose(false)}>Wróć do edycji</button>
        </div>
      )}

      <div className="fields">
        {senders && senders.length > 1 && init.mode === "new" && (
          <label><span>Od</span>
            <select value={from} disabled={sending} onChange={(e) => { setFrom(e.target.value); markDirty(); }}>
              {senders.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
            </select>
          </label>
        )}
        <label><span>Do</span>
          <input value={to} onChange={(e) => { setTo(e.target.value); markDirty(); }} placeholder="adres@example.com, drugi@example.com" disabled={sending} />
        </label>
        {showCc ? (
          <>
            <label><span>DW</span>
              <input value={cc} onChange={(e) => { setCc(e.target.value); markDirty(); }} disabled={sending} />
            </label>
            <label><span>UDW</span>
              <input value={bcc} onChange={(e) => { setBcc(e.target.value); markDirty(); }} disabled={sending} />
            </label>
          </>
        ) : (
          <button className="linklike plain" onClick={() => setShowCc(true)}>Dodaj DW / UDW</button>
        )}
        <label><span>Temat</span>
          <input value={subject} onChange={(e) => { setSubject(e.target.value); markDirty(); }} disabled={sending} />
        </label>
      </div>

      {init.note && <p className="note">{init.note}</p>}

      <RichEditor initialHtml={init.html} disabled={sending} apiRef={editorApi} onChange={(h) => { html.current = h; markDirty(); setSpellTick((t) => t + 1); }} />
      <SpellBar getHtml={() => html.current} tick={spellTick} api={editorApi} />

      {atts.length > 0 && (
        <ul className="att-list" aria-label="Załączniki">
          {atts.map((a, i) => (
            <li key={a.name + i}>
              {a.name} · {fmtSize(a.size)}
              <button className="linklike plain" disabled={sending}
                onClick={() => { setAtts((l) => l.filter((_, j) => j !== i)); markDirty(); }}>Usuń</button>
            </li>
          ))}
        </ul>
      )}

      {error && <p className="error pad" role="alert">{error}</p>}
      {canQueue && !sending && (
        <div className="queue-offer" role="status">
          <p>
            Nie ma połączenia z Gmail. Możesz zapisać wiadomość w kolejce – wyśle się sama, gdy wróci sieć (aplikacja
            musi być wtedy uruchomiona). Do tego czasu wiadomość leży na dysku w szyfrowanej bazie lokalnej.
          </p>
          <button onClick={() => void queue()}>Zapisz w kolejce do wysłania</button>
        </div>
      )}

      <footer className="compose-foot">
        <button className="primary" disabled={sending} onClick={() => void send()}>{sending ? "Wysyłanie…" : "Wyślij"}</button>
        <input ref={fileRef} type="file" multiple hidden onChange={(e) => void addFiles(e.target.files)} />
        <button disabled={sending} onClick={() => fileRef.current?.click()}>Dodaj załącznik</button>
        <span className="grow" />
        <button className="small warn" disabled={sending} onClick={() => void discard()}>
          {draftId ? "Usuń wersję roboczą" : "Odrzuć"}
        </button>
      </footer>
    </section>
  );
}
