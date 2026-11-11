import { useCallback, useEffect, useRef, useState } from "react";
import { ActIcon } from "./ActIcon";
import { api, errText } from "./api";
import { AddressInput } from "./AddressInput";
import { RichEditor, type EditorApi } from "./RichEditor";
import { SpellBar } from "./SpellBar";
import type { ComposeInit } from "./replyLogic";
import { sanitizeHtml } from "./sanitize";
import type { OutAttachment, OutgoingMessage } from "./types";
import { getUndo } from "./display";
import { fmtSize } from "./util";

const MAX_ATTACH_BYTES = 20 * 1024 * 1024;
const AUTOSAVE_MS = 20000;

interface Props {
  init: ComposeInit;
  onSent: (info: { threadId: string; warning?: string | null }) => void;
  /** Konto nadawcy: "gmail" albo "imap:adres". */
  from?: string;
  senders?: { id: string; label: string }[];
  onQueued: () => void;
  onScheduled: (atMs: number) => void;
  onClose: (draftSaved: boolean) => void;
}

interface Att extends OutAttachment {
  size: number;
}

const pad2 = (n: number) => String(n).padStart(2, "0");
/** Wartość dla pola datetime-local (czas lokalny). */
function toLocalInput(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}T${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}
function addHours(d: Date, h: number): Date {
  return new Date(d.getTime() + h * 3600_000);
}
/** Najbliższy ranek o 8:00: jutro (0) albo najbliższy poniedziałek po dzisiejszym dniu (1). */
function nextMorning(kind: 0 | 1): Date {
  const d = new Date();
  d.setHours(8, 0, 0, 0);
  d.setDate(d.getDate() + 1);
  if (kind === 1) while (d.getDay() !== 1) d.setDate(d.getDate() + 1);
  return d;
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

// Kopia robocza trafia do zaszyfrowanej bazy lokalnej (nie do localStorage). Brak bazy = brak kopii.
function backup(value: unknown | null) {
  const p = value === null ? api.composeBackupClear() : api.composeBackupSet(JSON.stringify(value));
  p.catch(() => { /* kopia lokalna jest dodatkiem */ });
}

export function Compose({ init, onSent, onQueued, onScheduled, onClose, from: fromProp, senders }: Props) {
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

  // Zapamiętuje adresy odbiorców w książce adresów (po udanej wysyłce lub zapisie w kolejce).
  const remember = () => { api.contactsRemember([to, cc, bcc].filter(Boolean).join(", ")).catch(() => {}); };
  // Krótka animacja „wysłano” przed zamknięciem okna (pomijana przy ograniczeniu ruchu w systemie).
  const [sentAnim, setSentAnim] = useState(false);
  const celebrate = async () => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    setSentAnim(true);
    await new Promise((r) => window.setTimeout(r, 1150));
  };

  // Cofnięcie wysyłki: po kliknięciu „Wyślij” wiadomość czeka kilka sekund w oknie, a „Cofnij” wraca do edycji.
  const [undoLeft, setUndoLeft] = useState<number | null>(null);
  const hasRecipient = () => Boolean(to.trim() || cc.trim() || bcc.trim());
  const send = () => {
    setError(null);
    if (!hasRecipient()) { setError("Podaj co najmniej jednego odbiorcę."); return; }
    const d = getUndo();
    if (d > 0) setUndoLeft(d); else void doSend();
  };
  useEffect(() => {
    if (undoLeft === null) return;
    if (undoLeft <= 0) { setUndoLeft(null); void doSend(); return; }
    const t = window.setTimeout(() => setUndoLeft((n) => (n === null ? null : n - 1)), 1000);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [undoLeft]);

  // Zaplanowana wysyłka: wiadomość czeka w zaszyfrowanej bazie lokalnej do wskazanej godziny.
  const [schedOpen, setSchedOpen] = useState(false);
  const [schedAt, setSchedAt] = useState("");
  const schedule = async (atMs: number) => {
    setError(null);
    if (!hasRecipient()) { setError("Podaj co najmniej jednego odbiorcę."); return; }
    if (!Number.isFinite(atMs)) { setError("Wybierz datę i godzinę."); return; }
    setSending(true);
    try {
      if (imapAcc) await api.imapScheduleMessage(imapAcc, build(), atMs);
      else await api.scheduleMessage(build(), atMs);
      remember();
      dirty.current = false;
      backup(null);
      onScheduled(atMs);
    } catch (e) {
      setError(errText(e));
      setSending(false);
    }
  };

  const doSend = async () => {
    setError(null);
    setSending(true);
    try {
      if (imapAcc) {
        const res = await api.imapSend(imapAcc, build());
        remember();
        if (draftRef.current) api.imapDeleteDraft(imapAcc, Number(draftRef.current)).catch(() => {});
        dirty.current = false;
        backup(null);
        await celebrate();
        onSent({ threadId: "", warning: res.warning });
        return;
      }
      const info = await api.sendMessage(build());
      remember();
      dirty.current = false;
      backup(null);
      await celebrate();
      onSent({ threadId: info.threadId || init.threadId || "" });
    } catch (e) {
      const m = errText(e);
      setError(m);
      // Kolejka tylko wtedy, gdy wysyłka na pewno się nie zaczęła (brak połączenia z serwerem).
      setCanQueue(imapAcc
        ? /^(Nie można połączyć z serwerem|Błąd bezpiecznego połączenia)/.test(m) || /^Serwer .* nie odpowiada\.$/.test(m)
        : m.startsWith("Brak połączenia"));
      setSending(false);
    }
  };

  const queue = async () => {
    setError(null);
    setSending(true);
    try {
      if (imapAcc) await api.imapOutboxQueue(imapAcc, build());
      else await api.queueMessage(build());
      remember();
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
        <button className="small" disabled={sending} title="Zapisz wersję roboczą" aria-label="Zapisz wersję roboczą" onClick={() => void saveDraft(true)}><ActIcon n="save" /></button>
        <button className="small" disabled={sending} title="Zamknij" aria-label="Zamknij" onClick={() => void requestClose()}><ActIcon n="clear" /></button>
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
          <AddressInput label="Do" value={to} onChange={(v) => { setTo(v); markDirty(); }} placeholder="adres@example.com, drugi@example.com" disabled={sending} />
        </label>
        {showCc ? (
          <>
            <label><span>DW</span>
              <AddressInput label="DW" value={cc} onChange={(v) => { setCc(v); markDirty(); }} disabled={sending} />
            </label>
            <label><span>UDW</span>
              <AddressInput label="UDW" value={bcc} onChange={(v) => { setBcc(v); markDirty(); }} disabled={sending} />
            </label>
          </>
        ) : (
          <button className="linklike plain" title="Dodaj DW / UDW" aria-label="Dodaj DW / UDW" onClick={() => setShowCc(true)}><ActIcon n="plus" /></button>
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
                title={`Usuń załącznik ${a.name}`} aria-label={`Usuń załącznik ${a.name}`} onClick={() => { setAtts((l) => l.filter((_, j) => j !== i)); markDirty(); }}><ActIcon n="trash" /></button>
            </li>
          ))}
        </ul>
      )}

      {error && <p className="error pad" role="alert">{error}</p>}
      {canQueue && !sending && (
        <div className="queue-offer" role="status">
          <p>
            {imapAcc ? "Nie ma połączenia z serwerem poczty." : "Nie ma połączenia z Gmail."} Możesz zapisać wiadomość w kolejce – wyśle się sama, gdy wróci sieć (aplikacja
            musi być wtedy uruchomiona). Do tego czasu wiadomość leży na dysku w szyfrowanej bazie lokalnej.
          </p>
          <button onClick={() => void queue()}>Zapisz w kolejce do wysłania</button>
        </div>
      )}

      {schedOpen && !sending && (
        <div className="sched-panel" role="group" aria-label="Zaplanuj wysyłkę">
          <span className="hint">Wyślij:</span>
          <button className="small" onClick={() => void schedule(addHours(new Date(), 1).getTime())}>za godzinę</button>
          <button className="small" onClick={() => void schedule(nextMorning(0).getTime())}>jutro o 8:00</button>
          <button className="small" onClick={() => void schedule(nextMorning(1).getTime())}>w poniedziałek o 8:00</button>
          <input type="datetime-local" value={schedAt} onChange={(e) => setSchedAt(e.target.value)} aria-label="Data i godzina wysyłki" />
          <button className="small primary" onClick={() => void schedule(new Date(schedAt).getTime())}>Zaplanuj</button>
        </div>
      )}

      {undoLeft !== null && (
        <div className="undo-send" role="alertdialog" aria-live="assertive">
          <div className="undo-card">
            <strong>Wysyłanie za {undoLeft} s</strong>
            <button className="primary" onClick={() => setUndoLeft(null)}>Cofnij</button>
          </div>
        </div>
      )}

      {sentAnim && (
        <div className="sent-anim" role="status" aria-live="polite">
          <div className="sent-card">
            <span className="sent-plane"><ActIcon n="send" /></span>
            <span className="sent-text">Wiadomość wysłana</span>
          </div>
        </div>
      )}
      <footer className="compose-foot">
        <button className="primary" disabled={sending} title={sending ? "Wysyłanie…" : "Wyślij"} aria-label={sending ? "Wysyłanie…" : "Wyślij"} aria-busy={sending} onClick={send}><ActIcon n="send" /></button>
        <button disabled={sending} title="Zaplanuj wysyłkę" aria-label="Zaplanuj wysyłkę" aria-expanded={schedOpen} onClick={() => { setSchedOpen((o) => !o); if (!schedAt) setSchedAt(toLocalInput(addHours(new Date(), 1))); }}><ActIcon n="clock" /></button>
        <input ref={fileRef} type="file" multiple hidden onChange={(e) => void addFiles(e.target.files)} />
        <button disabled={sending} title="Dodaj załącznik" aria-label="Dodaj załącznik" onClick={() => fileRef.current?.click()}><ActIcon n="attach" /></button>
        <span className="grow" />
        <button className="small warn" disabled={sending} title={draftId ? "Usuń wersję roboczą" : "Odrzuć wiadomość"} aria-label={draftId ? "Usuń wersję roboczą" : "Odrzuć wiadomość"} onClick={() => void discard()}>
          <ActIcon n="trash" />
        </button>
      </footer>
    </section>
  );
}
