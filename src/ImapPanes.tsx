import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { Attachments } from "./Attachments";
import { ReceiveButton } from "./ReceiveButton";
import { MessageBody } from "./MessageBody";
import type { ImapActOut, ImapFolder, ImapMsg, ImapRow, MessageView } from "./types";
import { commandFor, isTypingTarget } from "./shortcuts";
import { fmtDate, fmtFull } from "./util";
import { FOLDERS } from "./folders";
import { imapPrioKey, sortSearchResults } from "./prioSort";
import { prio, usePrioLevels } from "./prio";
import { PrioFlag, PrioMenu } from "./PrioUi";

const FOLDER_ICON = Object.fromEntries(FOLDERS.map((f) => [f.id, f])) as Record<string, (typeof FOLDERS)[number]>;
const OTHER_ICON = "M3 7h7l2 2h9v10H3z";

function Icon({ d }: { d: string }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}

/** Polskie nazwy i ikony jak w Gmailu. Pierwszy folder danej roli dostaje nazwę z Gmaila, kolejne zachowują własną. */
export function folderDisplay(folders: ImapFolder[]): Record<string, { label: string; icon: string }> {
  const out: Record<string, { label: string; icon: string }> = {};
  const used = new Set<string>();
  for (const f of folders) {
    let role = f.role;
    if (role === "other" && /(^|\/)(archive|archiwum)$/i.test(f.name)) role = "archive";
    const std = FOLDER_ICON[role];
    if (std && !used.has(role)) {
      used.add(role);
      out[f.name] = { label: std.label, icon: std.icon };
    } else {
      out[f.name] = { label: f.label.replace(/\//g, " › "), icon: std ? std.icon : OTHER_ICON };
    }
  }
  return out;
}

const emailOf = (from: string) => (/<([^>]+)>/.exec(from)?.[1] ?? from).trim().toLowerCase();

export function toView(m: ImapMsg): MessageView {
  return {
    id: String(m.uid), from: m.from, fromEmail: emailOf(m.from), mine: false, dateMs: m.dateMs,
    body: m.text, htmlOnly: !m.text && !!m.html, html: m.html, attachments: [], to: m.to, cc: m.cc,
    replyTo: m.replyTo, messageId: m.messageId, references: m.references, risk: { level: "none", score: 0, reasons: [] },
  };
}

/** Stan widoku jednego konta IMAP: foldery, bieżący folder i lista wiadomości. */
export function useImapView(account: string | null) {
  const [folders, setFolders] = useState<ImapFolder[]>([]);
  const [folder, setFolder] = useState("");
  const [rows, setRows] = useState<ImapRow[]>([]);
  const [next, setNext] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [term, setTerm] = useState("");
  const [counts, setCounts] = useState<Record<string, number>>({});
  const seq = useRef(0);
  const levels = usePrioLevels();

  // Wyniki wyszukiwania układają się na nowo, gdy zmieni się flaga.
  useEffect(() => {
    if (!term || !account || !folder) return;
    setRows((prev) => {
      const sorted = sortSearchResults(prev, (r) => r.dateMs, (r) => levels[imapPrioKey(account, folder, r.uid)] ?? 0);
      return sorted.every((r, i) => r === prev[i]) ? prev : sorted;
    });
  }, [levels, term, account, folder]);

  const loadCounts = useCallback((acc: string) => {
    api.imapCounts(acc).then((c) => setCounts(Object.fromEntries(c.map((x) => [x.name, x.unseen])))).catch(() => {});
  }, []);

  const load = useCallback(async (acc: string, name: string, offset: number, append: boolean) => {
    const my = ++seq.current;
    setLoading(true); setErr(null);
    try {
      const p = await api.imapList(acc, name, offset);
      if (my !== seq.current) return;
      await prio.ensure(p.rows.map((r) => imapPrioKey(acc, name, r.uid)));
      if (my !== seq.current) return;
      setRows((prev) => (append ? [...prev, ...p.rows] : p.rows));
      setNext(p.next);
    } catch (e) { if (my === seq.current) setErr(errText(e)); }
    finally { if (my === seq.current) setLoading(false); }
  }, []);

  useEffect(() => {
    seq.current++;
    setFolders([]); setFolder(""); setRows([]); setNext(null); setErr(null); setTerm(""); setCounts({});
    if (!account) return;
    const my = seq.current;
    setLoading(true);
    loadCounts(account);
    api.imapFolders(account).then((f) => {
      if (my !== seq.current) return;
      setFolders(f);
      const inbox = f.find((x) => x.role === "inbox") ?? f[0];
      if (inbox) { setFolder(inbox.name); void load(account, inbox.name, 0, false); } else setLoading(false);
    }).catch((e) => { if (my === seq.current) { setErr(errText(e)); setLoading(false); } });
  }, [account, load, loadCounts]);

  const search = async (q: string) => {
    if (!account || !folder || !q.trim()) return;
    const my = ++seq.current;
    setTerm(q.trim()); setLoading(true); setErr(null);
    try {
      const p = await api.imapSearch(account, folder, q.trim());
      if (my !== seq.current) return;
      await prio.ensure(p.rows.map((r) => imapPrioKey(account, folder, r.uid)));
      if (my !== seq.current) return;
      setRows(sortSearchResults(p.rows, (r) => r.dateMs, (r) => prio.get(imapPrioKey(account, folder, r.uid)))); setNext(null);
    } catch (e) { if (my === seq.current) setErr(errText(e)); }
    finally { if (my === seq.current) setLoading(false); }
  };

  return {
    account, folders, folder, rows, next, loading, err, term, counts,
    pick: (name: string) => { if (account) { setTerm(""); setFolder(name); void load(account, name, 0, false); } },
    refresh: () => { if (account && folder) { if (term) { void search(term); } else void load(account, folder, 0, false); loadCounts(account); } },
    search: (q: string) => void search(q),
    clearSearch: () => { setTerm(""); if (account && folder) void load(account, folder, 0, false); },
    more: () => { if (account && folder && next !== null) void load(account, folder, next, true); },
    patch: (uid: number, p: Partial<ImapRow>) => setRows((prev) => prev.map((r) => (r.uid === uid ? { ...r, ...p } : r))),
    remove: (uid: number) => setRows((prev) => prev.filter((r) => r.uid !== uid)),
  };
}
export type ImapView = ReturnType<typeof useImapView>;

export function ImapSidebarNav({ imap }: { imap: ImapView }) {
  return (
    <>
      {imap.folders.map((f) => {
        const d = folderDisplay(imap.folders)[f.name];
        return (
          <button key={f.name} className={"folder" + (f.name === imap.folder ? " active" : "")} onClick={() => imap.pick(f.name)}
            aria-current={f.name === imap.folder}>
            <Icon d={d.icon} />
            <span>{d.label}</span>
            {(imap.counts[f.name] ?? 0) > 0 && <span className={"badge" + (f.role === "inbox" ? "" : " muted")}>{imap.counts[f.name]}</span>}
          </button>
        );
      })}
      {imap.folders.length === 0 && !imap.loading && !imap.err && <p className="hint pad">Brak folderów.</p>}
    </>
  );
}

export function ImapListPane({ imap, role, openUid, selected, onRowClick, onSelectAll, onClear, onBulk }: {
  imap: ImapView; role: string; openUid: number | null; selected: Set<number>;
  onRowClick: (e: React.MouseEvent, r: ImapRow) => void; onSelectAll: () => void; onClear: () => void; onBulk: (action: string) => void;
}) {
  const label = imap.term ? `Wyniki: „${imap.term}”` : (folderDisplay(imap.folders)[imap.folder]?.label ?? "");
  const [q, setQ] = useState("");
  const levels = usePrioLevels();
  useEffect(() => { setQ(""); }, [imap.account, imap.folder]);
  return (
    <section className="list" aria-label="Lista wiadomości">
      <header className="list-head">
        <h2>{label}{imap.account ? ` · ${imap.account}` : ""}</h2>
        <ReceiveButton busy={imap.loading} onClick={imap.refresh} />
      </header>
      <form className="search" role="search" onSubmit={(e) => { e.preventDefault(); if (q.trim()) imap.search(q); }}>
        <input type="search" value={q} maxLength={300} placeholder="Szukaj w tym folderze (np. od:ala temat:faktura is:unread)" aria-label="Szukaj w folderze"
          onChange={(e) => setQ(e.target.value)} />
        <button className="small" type="submit">Szukaj</button>
        {imap.term && <button className="small" type="button" onClick={() => { setQ(""); imap.clearSearch(); }}>Wyczyść</button>}
      </form>
      {imap.err && <p className="error pad" role="alert">{imap.err}</p>}
      {selected.size > 0 && (
        <div className="bulkbar" role="toolbar" aria-label="Akcje na zaznaczonych">
          <strong>Zaznaczono: {selected.size}</strong>
          <button className="small" onClick={onSelectAll}>Wszystkie</button>
          {role === "inbox" && <button className="small" onClick={() => onBulk("archive")}>Archiwizuj</button>}
          <button className="small" onClick={() => onBulk(role === "trash" ? "inbox" : "trash")}>{role === "trash" ? "Przywróć" : "Usuń"}</button>
          <button className="small" onClick={() => onBulk(role === "spam" ? "inbox" : "spam")}>{role === "spam" ? "To nie spam" : "Spam"}</button>
          <button className="small" onClick={() => onBulk("read")}>Przeczytane</button>
          <button className="small" onClick={() => onBulk("unread")}>Nieprzeczytane</button>
          <button className="small" onClick={onClear}>Odznacz</button>
        </div>
      )}
      {!imap.loading && !imap.err && imap.rows.length === 0 && <div className="empty"><h3>Ten folder jest pusty</h3></div>}
      <ul>
        {imap.rows.map((r) => (
          <li key={r.uid} className={selected.has(r.uid) ? "picked" : ""}>
            <button className={"row" + (r.uid === openUid ? " active" : "") + (r.unread ? " unread" : "")}
              aria-pressed={selected.has(r.uid)} onClick={(e) => onRowClick(e, r)}>
              <span className="who">{r.from}</span>
              <span className="when"><PrioFlag level={levels[imapPrioKey(imap.account ?? "", imap.folder, r.uid)] ?? 0} />{r.starred ? "★ " : ""}{fmtDate(r.dateMs)}</span>
              <span className="subj">{r.subject || "(bez tematu)"}</span>
            </button>
          </li>
        ))}
      </ul>
      {imap.next !== null && <button className="more" disabled={imap.loading} onClick={imap.more}>{imap.loading ? "Wczytywanie…" : "Wczytaj więcej"}</button>}
    </section>
  );
}

interface ReaderProps {
  account: string;
  folder: string;
  /** Rola folderu: inbox / sent / trash / spam / drafts / archive / other. */
  role: string;
  uid: number | null;
  onCompose: (kind: "reply" | "replyAll" | "forward", msg: ImapMsg) => void;
  /** Czy skróty klawiszowe otwartej wiadomości są włączone (wyłączone, gdy na liście są zaznaczone wiadomości). */
  keys?: boolean;
  /** Po przeniesieniu wiadomości do innego folderu (kosz, spam, archiwum, skrzynka) – do komunikatu „Cofnij”. */
  onMoved?: (info: { action: string; folder: string; uid: number; out: ImapActOut }) => void;
  /** Otwarcie wersji roboczej do dalszej edycji. */
  onEditDraft?: (msg: ImapMsg) => void;
  /** Wywoływane po zmianie wiadomości (przeczytana, gwiazdka, usunięta…). */
  onChanged: (uid: number, patch: Partial<ImapRow> | "removed") => void;
}

export function ImapReader({ account, folder, role, uid, onChanged, onCompose, onMoved, onEditDraft, keys = true }: ReaderProps) {
  const [msg, setMsg] = useState<ImapMsg | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const changed = useRef(onChanged);
  changed.current = onChanged;
  const onMovedRef = useRef(onMoved);
  onMovedRef.current = onMoved;

  useEffect(() => {
    setMsg(null); setErr(null);
    if (uid === null) return;
    let live = true;
    setLoading(true);
    api.imapGet(account, folder, uid).then((m) => {
      if (!live) return;
      setMsg(m); changed.current(uid, { unread: false });
    }).catch((e) => { if (live) setErr(errText(e)); }).finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, [account, folder, uid]);

  const act = async (action: string) => {
    if (!msg) return;
    setErr(null);
    try {
      const out = await api.imapAct(account, folder, msg.uid, action);
      if (["trash", "spam", "archive", "inbox"].includes(action)) {
        changed.current(msg.uid, "removed");
        onMovedRef.current?.({ action, folder, uid: msg.uid, out });
        setMsg(null);
        return;
      }
      const patch: Partial<ImapRow> = {};
      if (action === "read") patch.unread = false;
      if (action === "unread") patch.unread = true;
      if (action === "star") patch.starred = true;
      if (action === "unstar") patch.starred = false;
      setMsg({ ...msg, unread: patch.unread ?? msg.unread, starred: patch.starred ?? msg.starred });
      changed.current(msg.uid, patch);
    } catch (e) { setErr(errText(e)); }
  };

  // Skróty klawiszowe dla otwartej wiadomości IMAP (działają tylko na tym koncie i tej wiadomości).
  const keyRef = useRef<(e: KeyboardEvent) => void>(() => {});
  keyRef.current = (e: KeyboardEvent) => {
    if (!keys || !msg || isTypingTarget(e.target as HTMLElement | null) || document.querySelector(".modal-back")) return;
    const cmd = commandFor(e);
    if (!cmd) return;
    const run = (f: () => void) => { e.preventDefault(); f(); };
    switch (cmd) {
      case "trash": run(() => void act(role === "trash" ? "inbox" : "trash")); break;
      case "spam": run(() => void act(role === "spam" ? "inbox" : "spam")); break;
      case "archive": if (role === "inbox") run(() => void act("archive")); break;
      case "read": run(() => void act("read")); break;
      case "unread": run(() => void act("unread")); break;
      case "star": run(() => void act(msg.starred ? "unstar" : "star")); break;
      case "reply": run(() => onCompose("reply", msg)); break;
      case "replyAll": run(() => onCompose("replyAll", msg)); break;
      case "forward": run(() => onCompose("forward", msg)); break;
    }
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keyRef.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  return (
    <main className="reader" aria-label="Wiadomość">
      {err && <p className="error pad" role="alert">{err}</p>}
      {loading && <div className="empty"><p>Wczytywanie wiadomości…</p></div>}
      {!loading && !msg && !err && <div className="empty"><p>Wybierz wiadomość z listy.</p></div>}
      {msg && (
        <>
          <header className="reader-head">
            <h2>{msg.subject || "(bez tematu)"}</h2>
            <div className="tools actions" role="toolbar" aria-label="Akcje na wiadomości">
              {role === "drafts" && onEditDraft && <button className="primary" onClick={() => onEditDraft(msg)}>Edytuj wersję roboczą</button>}
              <button className={role === "drafts" ? "" : "primary"} onClick={() => onCompose("reply", msg)}>Odpowiedz</button>
              <button onClick={() => onCompose("replyAll", msg)}>Odpowiedz wszystkim</button>
              <button onClick={() => onCompose("forward", msg)}>Przekaż dalej</button>
              <span className="sep" />
              <button onClick={() => void act(msg.unread ? "read" : "unread")}>{msg.unread ? "Oznacz jako przeczytaną" : "Oznacz jako nieprzeczytaną"}</button>
              <button onClick={() => void act(msg.starred ? "unstar" : "star")}>{msg.starred ? "Usuń gwiazdkę" : "Gwiazdka"}</button>
              <PrioMenu itemKey={imapPrioKey(account, folder, msg.uid)} onError={(m) => setErr(m)} />
              {role === "inbox" && <button onClick={() => void act("archive")}>Archiwizuj</button>}
              {!["inbox", "trash", "spam"].includes(role) && <button onClick={() => void act("inbox")}>Do skrzynki</button>}
              {role === "spam" ? <button onClick={() => void act("inbox")}>To nie spam</button> : <button onClick={() => void act("spam")}>Spam</button>}
              {role === "trash" ? <button onClick={() => void act("inbox")}>Przywróć z kosza</button> : <button onClick={() => void act("trash")}>Usuń</button>}
            </div>
          </header>
          <div className="thread">
            <article className="bubble received wide">
              <div className="bubble-head">
                <strong>{msg.from}</strong>
                <time>{fmtFull(msg.dateMs)}</time>
              </div>
              <p className="hint">Do: {msg.to}{msg.cc ? ` · Kopia: ${msg.cc}` : ""}</p>
              <MessageBody key={msg.uid} m={toView(msg)} strict={false} />
              <Attachments
                m={{ ...toView(msg), attachments: msg.attachments }}
                strict={false}
                dl={(a) => api.imapDownload(account, folder, msg.uid, Number(a.id), a.name)}
              />
            </article>
          </div>
        </>
      )}
    </main>
  );
}
