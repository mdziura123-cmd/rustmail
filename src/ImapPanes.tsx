import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { MessageBody } from "./MessageBody";
import type { ImapFolder, ImapMsg, ImapRow, MessageView } from "./types";
import { fmtDate, fmtFull } from "./util";
import { FOLDERS } from "./folders";

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

function toView(m: ImapMsg): MessageView {
  return {
    id: String(m.uid), from: m.from, fromEmail: emailOf(m.from), mine: false, dateMs: m.dateMs,
    body: m.text, htmlOnly: !m.text && !!m.html, html: m.html, attachments: [], to: m.to, cc: m.cc,
    replyTo: "", messageId: "", references: "", risk: { level: "none", score: 0, reasons: [] },
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

  const loadCounts = useCallback((acc: string) => {
    api.imapCounts(acc).then((c) => setCounts(Object.fromEntries(c.map((x) => [x.name, x.unseen])))).catch(() => {});
  }, []);

  const load = useCallback(async (acc: string, name: string, offset: number, append: boolean) => {
    const my = ++seq.current;
    setLoading(true); setErr(null);
    try {
      const p = await api.imapList(acc, name, offset);
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
      setRows(p.rows); setNext(null);
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

/** Odebrane ze wszystkich kont IMAP naraz (do widoku „Wszystkie skrzynki”). */
export function useImapInboxes(accounts: string[], enabled: boolean) {
  const [items, setItems] = useState<{ account: string; row: ImapRow }[]>([]);
  const [error, setError] = useState<string | null>(null);
  const key = accounts.join("\n");
  const refresh = useCallback(async () => {
    if (!enabled || accounts.length === 0) { setItems([]); return; }
    const errs: string[] = [];
    const all = await Promise.all(accounts.map(async (account) => {
      try { return (await api.imapList(account, "INBOX", 0)).rows.map((row) => ({ account, row })); }
      catch (e) { errs.push(`${account}: ${errText(e)}`); return []; }
    }));
    setItems(all.flat());
    setError(errs.length ? errs.join(" ") : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, enabled]);
  useEffect(() => { void refresh(); }, [refresh]);
  return {
    items, error, refresh,
    patch: (account: string, uid: number, pt: Partial<ImapRow>) =>
      setItems((prev) => prev.map((x) => (x.account === account && x.row.uid === uid ? { ...x, row: { ...x.row, ...pt } } : x))),
    remove: (account: string, uid: number) => setItems((prev) => prev.filter((x) => !(x.account === account && x.row.uid === uid))),
  };
}

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

export function ImapListPane({ imap, openUid, onOpen }: { imap: ImapView; openUid: number | null; onOpen: (r: ImapRow) => void }) {
  const label = imap.term ? `Wyniki: „${imap.term}”` : (folderDisplay(imap.folders)[imap.folder]?.label ?? "");
  const [q, setQ] = useState("");
  useEffect(() => { setQ(""); }, [imap.account, imap.folder]);
  return (
    <section className="list" aria-label="Lista wiadomości">
      <header className="list-head">
        <h2>{label}{imap.account ? ` · ${imap.account}` : ""}</h2>
        <button className="small" disabled={imap.loading} onClick={imap.refresh}>{imap.loading ? "Wczytywanie…" : "Odśwież"}</button>
      </header>
      <form className="search" role="search" onSubmit={(e) => { e.preventDefault(); if (q.trim()) imap.search(q); }}>
        <input type="search" value={q} maxLength={100} placeholder="Szukaj w tym folderze" aria-label="Szukaj w folderze"
          onChange={(e) => setQ(e.target.value)} />
        <button className="small" type="submit">Szukaj</button>
        {imap.term && <button className="small" type="button" onClick={() => { setQ(""); imap.clearSearch(); }}>Wyczyść</button>}
      </form>
      {imap.err && <p className="error pad" role="alert">{imap.err}</p>}
      {!imap.loading && !imap.err && imap.rows.length === 0 && <div className="empty"><h3>Ten folder jest pusty</h3></div>}
      <ul>
        {imap.rows.map((r) => (
          <li key={r.uid}>
            <button className={"row" + (r.uid === openUid ? " active" : "") + (r.unread ? " unread" : "")} onClick={() => onOpen(r)}>
              <span className="who">{r.from}</span>
              <span className="when">{r.starred ? "★ " : ""}{fmtDate(r.dateMs)}</span>
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
  /** Wywoływane po zmianie wiadomości (przeczytana, gwiazdka, usunięta…). */
  onChanged: (uid: number, patch: Partial<ImapRow> | "removed") => void;
}

export function ImapReader({ account, folder, role, uid, onChanged }: ReaderProps) {
  const [msg, setMsg] = useState<ImapMsg | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const changed = useRef(onChanged);
  changed.current = onChanged;

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
      await api.imapAct(account, folder, msg.uid, action);
      if (["trash", "spam", "archive", "inbox"].includes(action)) { changed.current(msg.uid, "removed"); setMsg(null); return; }
      const patch: Partial<ImapRow> = {};
      if (action === "read") patch.unread = false;
      if (action === "unread") patch.unread = true;
      if (action === "star") patch.starred = true;
      if (action === "unstar") patch.starred = false;
      setMsg({ ...msg, unread: patch.unread ?? msg.unread, starred: patch.starred ?? msg.starred });
      changed.current(msg.uid, patch);
    } catch (e) { setErr(errText(e)); }
  };

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
              <button onClick={() => void act(msg.unread ? "read" : "unread")}>{msg.unread ? "Oznacz jako przeczytaną" : "Oznacz jako nieprzeczytaną"}</button>
              <button onClick={() => void act(msg.starred ? "unstar" : "star")}>{msg.starred ? "Usuń gwiazdkę" : "Gwiazdka"}</button>
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
              {msg.attachments.length > 0 && (
                <p className="hint">Załączniki (pobieranie jeszcze niedostępne): {msg.attachments.map((a) => a.name).join(", ")}</p>
              )}
            </article>
          </div>
          <p className="hint pad">Odpowiadanie i wysyłanie z kont IMAP nie jest jeszcze dostępne.</p>
        </>
      )}
    </main>
  );
}
