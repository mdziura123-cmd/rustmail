import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { Attachments } from "./Attachments";
import { ReceiveButton } from "./ReceiveButton";
import { MessageBody } from "./MessageBody";
import type { ImapActOut, ImapFolder, ImapMsg, ImapRow, MessageView } from "./types";
import { commandFor, isTypingTarget } from "./shortcuts";
import { fmtDate, fmtFull } from "./util";
import { ActIcon } from "./ActIcon";
import { useContextMenu, type MenuEntry } from "./ContextMenu";
import { FOLDERS } from "./folders";
import { imapPrioKey, sortSearchResults } from "./prioSort";
import { prio, usePrioLevels } from "./prio";
import { useFlagsFirst, flagsFirst } from "./listPrefs";
import { flagMatches, type FlagFilter } from "./searchQuery";
import { FlagsFirstToggle, PrioFlag, PrioMenu } from "./PrioUi";

const FOLDER_ICON = Object.fromEntries(FOLDERS.map((f) => [f.id, f])) as Record<string, (typeof FOLDERS)[number]>;
const OTHER_ICON = "M3 7h7l2 2h9v10H3z";

const UNREAD_ICON = "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z";

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
  const ff = useFlagsFirst();
  const flagRef = useRef<FlagFilter>("any");
  const [unreadView, setUnreadView] = useState(false);
  const rowsRef = useRef<ImapRow[]>([]);
  rowsRef.current = rows;
  const countsTimer = useRef(0);

  // Wyniki wyszukiwania (albo cała lista przy włączonym „flagi na górze”) układają się na nowo, gdy zmieni się flaga.
  useEffect(() => {
    if (!account || !folder) return;
    setRows((prev) => {
      const flagged = Boolean(term) || ff;
      const sorted = sortSearchResults(prev, (r) => r.dateMs, (r) => (flagged ? levels[imapPrioKey(account, folder, r.uid)] ?? 0 : 0));
      return sorted.every((r, i) => r === prev[i]) ? prev : sorted;
    });
  }, [levels, term, ff, account, folder]);

  // Licznik nieprzeczytanych zmienia się od razu, a po chwili jest odświeżany z serwera (prawdziwa wartość).
  const bumpCount = (name: string, d: number) => {
    setCounts((c) => (name in c ? { ...c, [name]: Math.max(0, c[name] + d) } : c));
    window.clearTimeout(countsTimer.current);
    if (account) countsTimer.current = window.setTimeout(() => loadCounts(account), 1500);
  };
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
      const lv = (r: ImapRow) => prio.get(imapPrioKey(acc, name, r.uid));
      const ord = (l: ImapRow[]) => (flagsFirst() ? sortSearchResults(l, (r) => r.dateMs, lv) : l);
      setRows((prev) => ord(append ? [...prev, ...p.rows] : p.rows));
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

  const search = async (q: string, flag: FlagFilter = flagRef.current, inFolder: string = folder, unreadTab = false) => {
    if (!account || !inFolder || !q.trim()) return;
    flagRef.current = flag;
    setUnreadView(unreadTab);
    const my = ++seq.current;
    setTerm(q.trim()); setLoading(true); setErr(null);
    try {
      const att = q.trim().match(/^za[łl]acznik:\s*(.+)$/i);
      const p = att ? await api.imapSearchAttachments(account, inFolder, att[1]) : await api.imapSearch(account, inFolder, q.trim());
      if (my !== seq.current) return;
      await prio.ensure(p.rows.map((r) => imapPrioKey(account, inFolder, r.uid)));
      if (my !== seq.current) return;
      const lv = (r: ImapRow) => prio.get(imapPrioKey(account, inFolder, r.uid));
      setRows(sortSearchResults(p.rows.filter((r) => flagMatches(lv(r), flag)), (r) => r.dateMs, lv)); setNext(null);
    } catch (e) { if (my === seq.current) setErr(errText(e)); }
    finally { if (my === seq.current) setLoading(false); }
  };

  return {
    account, folders, folder, rows, next, loading, err, term, counts, unreadView,
    pickUnread: () => {
      const inbox = folders.find((x) => x.role === "inbox");
      if (account && inbox) { setFolder(inbox.name); void search("is:unread", "any", inbox.name, true); }
    },
    pick: (name: string) => { if (account) { setUnreadView(false); setTerm(""); setFolder(name); void load(account, name, 0, false); } },
    refresh: () => { if (account && folder) { if (term) { void search(term); } else void load(account, folder, 0, false); loadCounts(account); } },
    search: (q: string, flag: FlagFilter = "any") => void search(q, flag),
    clearSearch: () => { setTerm(""); setUnreadView(false); flagRef.current = "any"; if (account && folder) void load(account, folder, 0, false); },
    more: () => { if (account && folder && next !== null) void load(account, folder, next, true); },
    patch: (uid: number, p: Partial<ImapRow>) => {
      const old = rowsRef.current.find((r) => r.uid === uid);
      setRows((prev) => prev.map((r) => (r.uid === uid ? { ...r, ...p } : r)));
      if (old && p.unread !== undefined && p.unread !== old.unread) bumpCount(folder, p.unread ? 1 : -1);
    },
    remove: (uid: number) => {
      const old = rowsRef.current.find((r) => r.uid === uid);
      setRows((prev) => prev.filter((r) => r.uid !== uid));
      if (old?.unread) bumpCount(folder, -1);
    },
  };
}
export type ImapView = ReturnType<typeof useImapView>;

export function ImapSidebarNav({ imap }: { imap: ImapView }) {
  return (
    <>
      {imap.folders.map((f) => {
        const d = folderDisplay(imap.folders)[f.name];
        const here = f.name === imap.folder && !imap.unreadView;
        return (
          <Fragment key={f.name}>
            <button className={"folder" + (here ? " active" : "")} onClick={() => imap.pick(f.name)} aria-current={here}>
              <Icon d={d.icon} />
              <span>{d.label}</span>
              {(imap.counts[f.name] ?? 0) > 0 && <span className={"badge" + (f.role === "inbox" ? "" : " muted")}>{imap.counts[f.name]}</span>}
            </button>
            {f.role === "inbox" && (
              <button className={"folder" + (imap.unreadView ? " active" : "")} onClick={imap.pickUnread} aria-current={imap.unreadView}>
                <Icon d={UNREAD_ICON} />
                <span>Nieprzeczytane</span>
              </button>
            )}
          </Fragment>
        );
      })}
      {imap.folders.length === 0 && !imap.loading && !imap.err && <p className="hint pad">Brak folderów.</p>}
    </>
  );
}

export function ImapListPane({ imap, role, openUid, selected, onRowClick, onSelectAll, onClear, onBulk, rowMenu }: {
  imap: ImapView; role: string; openUid: number | null; selected: Set<number>;
  onRowClick: (e: React.MouseEvent, r: ImapRow) => void; onSelectAll: () => void; onClear: () => void; onBulk: (action: string) => void;
  /** Pozycje menu prawego przycisku dla wiadomości. */
  rowMenu: (r: ImapRow) => MenuEntry[];
}) {
  const cm = useContextMenu();
  const label = imap.unreadView ? "Nieprzeczytane" : imap.term ? `Wyniki: „${imap.term}”` : (folderDisplay(imap.folders)[imap.folder]?.label ?? "");
  const levels = usePrioLevels();
  return (
    <section className="list" aria-label="Lista wiadomości">
      <header className="list-head">
        <h2>{label}{imap.account ? ` · ${imap.account}` : ""}</h2>
        <FlagsFirstToggle />
        <ReceiveButton busy={imap.loading} onClick={imap.refresh} />
      </header>
      {imap.err && <p className="error pad" role="alert">{imap.err}</p>}
      {selected.size > 0 && (
        <div className="bulkbar" role="toolbar" aria-label="Akcje na zaznaczonych">
          <strong>Zaznaczono: {selected.size}</strong>
          <button className="small" title="Zaznacz wszystkie na liście (Ctrl+A)" aria-label="Zaznacz wszystkie na liście (Ctrl+A)" onClick={onSelectAll}><ActIcon n="selectAll"/></button>
          {role === "inbox" && <button className="small" title="Archiwizuj zaznaczone (e)" aria-label="Archiwizuj zaznaczone (e)" onClick={() => onBulk("archive")}><ActIcon n="archive"/></button>}
          <button className="small" title={role === "trash" ? "Przywróć zaznaczone z kosza" : "Usuń zaznaczone – przenieś do kosza (Delete)"} aria-label={role === "trash" ? "Przywróć zaznaczone z kosza" : "Usuń zaznaczone – przenieś do kosza (Delete)"} onClick={() => onBulk(role === "trash" ? "inbox" : "trash")}><ActIcon n={role === "trash" ? "restore" : "trash"} /></button>
          <button className="small" title={role === "spam" ? "To nie spam – przenieś zaznaczone do skrzynki" : "Oznacz zaznaczone jako spam (!)"} aria-label={role === "spam" ? "To nie spam – przenieś zaznaczone do skrzynki" : "Oznacz zaznaczone jako spam (!)"} onClick={() => onBulk(role === "spam" ? "inbox" : "spam")}><ActIcon n={role === "spam" ? "notspam" : "spam"} /></button>
          <button className="small" title="Oznacz zaznaczone jako przeczytane (Shift+I)" aria-label="Oznacz zaznaczone jako przeczytane (Shift+I)" onClick={() => onBulk("read")}><ActIcon n="read"/></button>
          <button className="small" title="Oznacz zaznaczone jako nieprzeczytane (Shift+U)" aria-label="Oznacz zaznaczone jako nieprzeczytane (Shift+U)" onClick={() => onBulk("unread")}><ActIcon n="unread"/></button>
          <button className="small" title="Odznacz wszystko (Esc)" aria-label="Odznacz wszystko (Esc)" onClick={onClear}><ActIcon n="clear"/></button>
        </div>
      )}
      {!imap.loading && !imap.err && imap.rows.length === 0 && <div className="empty"><h3>Ten folder jest pusty</h3></div>}
      <ul>
        {imap.rows.map((r) => (
          <li key={r.uid} className={selected.has(r.uid) ? "picked" : ""}>
            <button className={"row" + (r.uid === openUid ? " active" : "") + (r.unread ? " unread" : "")}
              aria-pressed={selected.has(r.uid)} onClick={(e) => onRowClick(e, r)} {...cm.bind(() => rowMenu(r))}>
              <span className="who">{r.unread && <i className="udot" role="img" aria-label="Nieprzeczytana" />}{r.from}</span>
              <span className="when"><PrioFlag level={levels[imapPrioKey(imap.account ?? "", imap.folder, r.uid)] ?? 0} />{r.starred ? "★ " : ""}{fmtDate(r.dateMs)}</span>
              <span className="subj">{r.subject || "(bez tematu)"}</span>
            </button>
          </li>
        ))}
      </ul>
      {imap.next !== null && <button className="more" disabled={imap.loading} onClick={imap.more}>{imap.loading ? "Wczytywanie…" : "Wczytaj więcej"}</button>}
      {cm.menu}
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
  /** „Do kalendarza”: temat i treść wiadomości. */
  onCalendar?: (subject: string, text: string) => void;
  /** Czy skróty klawiszowe otwartej wiadomości są włączone (wyłączone, gdy na liście są zaznaczone wiadomości). */
  keys?: boolean;
  /** Po przeniesieniu wiadomości do innego folderu (kosz, spam, archiwum, skrzynka) – do komunikatu „Cofnij”. */
  onMoved?: (info: { action: string; folder: string; uid: number; out: ImapActOut }) => void;
  /** Otwarcie wersji roboczej do dalszej edycji. */
  onEditDraft?: (msg: ImapMsg) => void;
  /** Polecenie z menu prawego przycisku do wykonania, gdy wiadomość się wczyta (odpowiedz, przekaż dalej). */
  pending?: { uid: number; cmd: "reply" | "replyAll" | "forward" } | null;
  onPendingDone?: () => void;
  /** Wywoływane po zmianie wiadomości (przeczytana, gwiazdka, usunięta…). */
  onChanged: (uid: number, patch: Partial<ImapRow> | "removed") => void;
}

export function ImapReader({ account, folder, role, uid, onChanged, onCompose, onCalendar, onMoved, onEditDraft, pending, onPendingDone, keys = true }: ReaderProps) {
  const [msg, setMsg] = useState<ImapMsg | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (pending && msg && msg.uid === pending.uid) { onCompose(pending.cmd, msg); onPendingDone?.(); }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending, msg]);
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
      case "prio0": case "prio1": case "prio2": case "prio3":
        run(() => { prio.set(imapPrioKey(account, folder, msg.uid), Number(cmd.slice(4))).catch(() => setErr("Nie udało się zapisać flagi.")); }); break;
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
              {role === "drafts" && onEditDraft && <button title="Edytuj wersję roboczą" aria-label="Edytuj wersję roboczą" className="primary" onClick={() => onEditDraft(msg)}><ActIcon n="edit"/></button>}
              <button title="Odpowiedz nadawcy (r)" aria-label="Odpowiedz nadawcy (r)" className={role === "drafts" ? "" : "primary"} onClick={() => onCompose("reply", msg)}><ActIcon n="reply"/></button>
              <button title="Odpowiedz wszystkim (a)" aria-label="Odpowiedz wszystkim (a)" onClick={() => onCompose("replyAll", msg)}><ActIcon n="replyAll"/></button>
              <button title="Przekaż dalej (f)" aria-label="Przekaż dalej (f)" onClick={() => onCompose("forward", msg)}><ActIcon n="forward"/></button>
              {onCalendar && <button title="Dodaj do kalendarza" aria-label="Dodaj do kalendarza" onClick={() => onCalendar(msg.subject, msg.text || "")}><ActIcon n="calendar"/></button>}
              <span className="sep" />
              <button title={msg.unread ? "Oznacz jako przeczytaną (Shift+I)" : "Oznacz jako nieprzeczytaną (Shift+U)"} aria-label={msg.unread ? "Oznacz jako przeczytaną (Shift+I)" : "Oznacz jako nieprzeczytaną (Shift+U)"} onClick={() => void act(msg.unread ? "read" : "unread")}><ActIcon n={msg.unread ? "read" : "unread"} /></button>
              <button title={msg.starred ? "Usuń gwiazdkę (s)" : "Dodaj gwiazdkę (s)"} aria-label={msg.starred ? "Usuń gwiazdkę (s)" : "Dodaj gwiazdkę (s)"} onClick={() => void act(msg.starred ? "unstar" : "star")}><ActIcon n="star" filled={msg.starred} /></button>
              <PrioMenu itemKey={imapPrioKey(account, folder, msg.uid)} onError={(m) => setErr(m)} />
              {role === "inbox" && <button title="Archiwizuj (e)" aria-label="Archiwizuj (e)" onClick={() => void act("archive")}><ActIcon n="archive"/></button>}
              {!["inbox", "trash", "spam"].includes(role) && <button title="Przenieś do skrzynki odbiorczej" aria-label="Przenieś do skrzynki odbiorczej" onClick={() => void act("inbox")}><ActIcon n="inbox"/></button>}
              {role === "spam" ? <button title="To nie jest spam – przenieś do skrzynki" aria-label="To nie jest spam – przenieś do skrzynki" onClick={() => void act("inbox")}><ActIcon n="notspam"/></button> : <button title="Oznacz jako spam (!)" aria-label="Oznacz jako spam (!)" onClick={() => void act("spam")}><ActIcon n="spam"/></button>}
              {role === "trash" ? <button title="Przywróć z kosza" aria-label="Przywróć z kosza" onClick={() => void act("inbox")}><ActIcon n="restore"/></button> : <button title="Usuń – przenieś do kosza (Delete)" aria-label="Usuń – przenieś do kosza (Delete)" onClick={() => void act("trash")}><ActIcon n="trash"/></button>}
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
                pv={(a) => api.imapPreview(account, folder, msg.uid, Number(a.id))}
              />
            </article>
          </div>
        </>
      )}
    </main>
  );
}
