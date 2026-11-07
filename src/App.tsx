import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { Attachments } from "./Attachments";
import { AuthPanel } from "./AuthPanel";
import { CacheDialog } from "./CacheDialog";
import { SettingsDialog } from "./SettingsDialog";
import { BACKUP_KEY, Compose } from "./Compose";
import { MessageBody } from "./MessageBody";
import { RiskBanner } from "./RiskBanner";
import { splitQuoted } from "./quoteTrim";
import { SenderLists } from "./SenderLists";
import { SignatureDialog } from "./SignatureDialog";
import { buildForward, buildNew, buildReply, replyTarget, type ComposeInit } from "./replyLogic";
import { FOLDERS, type FolderId } from "./folders";
import { UpdateBanner } from "./UpdateBanner";
import { useUpdater } from "./useUpdater";
import { fmtDate, fmtFull } from "./util";
import type { AppInfo, AuthStatus, LabelInfo, ThreadAction, ThreadDetail, ThreadSummary } from "./types";
import "./styles.css";
import "./compose.css";

type Theme = "auto" | "light" | "dark";
const LS = { theme: "rm.theme", left: "rm.left", mid: "rm.mid", auto: "rm.autoupdate", notify: "rm.notify", content: "rm.notifycontent" };
const STAR = "M12 3l2.9 6 6.6.9-4.8 4.6 1.2 6.5L12 17.8 6.1 21l1.2-6.5L2.5 9.9 9.1 9z";

function load(key: string, fallback: string): string {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}
function save(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* pamięć niedostępna – ignorujemy */ }
}
const INVERSE: Partial<Record<ThreadAction, ThreadAction>> = {
  archive: "unarchive", unarchive: "archive", spam: "notspam", notspam: "spam", trash: "untrash", untrash: "trash",
};
const ACTION_DONE: Partial<Record<ThreadAction, string>> = {
  archive: "Zarchiwizowano konwersację.", unarchive: "Przeniesiono do skrzynki odbiorczej.",
  spam: "Oznaczono jako spam.", notspam: "Oznaczono jako niebędące spamem.",
  trash: "Przeniesiono do kosza.", untrash: "Przywrócono z kosza.",
};
interface ComposeState { key: number; init: ComposeInit }
interface Toast { key: number; text: string; undo?: () => void }
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

function Icon({ d }: { d: string }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}

function useResizer(key: string, initial: number, lo: number, hi: number) {
  const [width, setWidth] = useState(() => clamp(Number(load(key, String(initial))) || initial, lo, hi));
  const onPointerDown = useCallback((e: React.PointerEvent) => {
    const startX = e.clientX;
    const startW = width;
    let last = startW;
    const move = (ev: PointerEvent) => {
      last = clamp(startW + (ev.clientX - startX), lo, hi);
      setWidth(last);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      save(key, String(last));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }, [width, key, lo, hi]);
  return { width, onPointerDown };
}

export default function App() {
  const [theme, setTheme] = useState<Theme>(() => load(LS.theme, "auto") as Theme);
  const [folder, setFolder] = useState<FolderId>("inbox");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [nextToken, setNextToken] = useState<string | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ThreadDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [autoUpdate, setAutoUpdate] = useState(() => load(LS.auto, "1") === "1");
  const [confirmRollback, setConfirmRollback] = useState(false);
  const [compose, setCompose] = useState<ComposeState | null>(null);
  const [signature, setSignature] = useState("");
  const [sigOpen, setSigOpen] = useState(false);
  const [senderDlg, setSenderDlg] = useState<{ email: string | null } | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [restore, setRestore] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [searchTerm, setSearchTerm] = useState<string | null>(null);
  const [listOffline, setListOffline] = useState(false);
  const [cacheOpen, setCacheOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [notifyOn, setNotifyOn] = useState(() => load(LS.notify, "0") === "1");
  const [notifyContent, setNotifyContent] = useState(() => load(LS.content, "0") === "1");
  const [stats, setStats] = useState<{ inbox: number; spam: number; labels: LabelInfo[] }>({ inbox: 0, spam: 0, labels: [] });
  const [newLabel, setNewLabel] = useState<string | null>(null);
  const searchRef = useRef<string | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);
  const composeKey = useRef(0);
  const updater = useUpdater(autoUpdate);
  const left = useResizer(LS.left, 240, 180, 360);
  const mid = useResizer(LS.mid, 360, 260, 560);
  const listReq = useRef(0);
  const threadReq = useRef(0);
  const mounted = useRef(false);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "auto") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    if (mounted.current) save(LS.theme, theme);
    mounted.current = true;
  }, [theme]);

  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => setInfo(null));
  }, []);

  const refreshAuth = useCallback(async () => {
    try {
      setAuth(await api.authStatus());
    } catch (e) {
      setAuthError(errText(e));
      setAuth({ configured: false, builtin: false, email: null });
    }
  }, []);
  useEffect(() => { void refreshAuth(); }, [refreshAuth]);

  const loadList = useCallback(async (f: FolderId, token: string | null, append: boolean) => {
    const my = ++listReq.current;
    setLoadingList(true);
    setListError(null);
    try {
      const q = searchRef.current;
      const page = q ? await api.searchThreads(q, token) : await api.listThreads(f, token);
      if (my !== listReq.current) return;
      setThreads((prev) => (append ? [...prev, ...page.threads] : page.threads));
      setNextToken(page.nextPageToken);
      setListOffline(Boolean(page.offline));
    } catch (e) {
      if (my === listReq.current) setListError(errText(e));
    } finally {
      if (my === listReq.current) setLoadingList(false);
    }
  }, []);

  const email = auth?.email ?? null;
  useEffect(() => {
    if (!email) return;
    setThreads([]);
    setNextToken(null);
    setOpenId(null);
    setDetail(null);
    void loadList(folder, null, false);
  }, [email, folder, loadList]);

  const needsReauth = Boolean(auth?.needsReauth);

  const showToast = useCallback((text: string, undo?: () => void) => {
    window.clearTimeout(toastTimer.current);
    setToast({ key: Date.now(), text, undo });
    toastTimer.current = window.setTimeout(() => setToast(null), 9000);
  }, []);

  const loadDetail = useCallback(async (id: string) => {
    const my = ++threadReq.current;
    setOpenId(id);
    setDetail(null);
    setDetailError(null);
    setDetailLoading(true);
    setCollapsed(new Set());
    try {
      const d = await api.getThread(id);
      if (my !== threadReq.current) return;
      setDetail(d);
      if (d.unread && !needsReauth && !d.offline) {
        // jak w Outlooku: otwarcie konwersacji oznacza ją jako przeczytaną
        api.threadAction(id, "read").then(() => {
          setDetail((cur) => (cur && cur.id === id ? { ...cur, unread: false } : cur));
          setThreads((prev) => prev.map((t) => (t.id === id ? { ...t, unread: false } : t)));
        }).catch(() => { /* brak uprawnień lub sieci – konwersacja zostaje nieprzeczytana */ });
      }
    } catch (e) {
      if (my === threadReq.current) setDetailError(errText(e));
    } finally {
      if (my === threadReq.current) setDetailLoading(false);
    }
  }, [needsReauth]);

  const startCompose = (init: ComposeInit) => {
    if (compose) {
      showToast("Najpierw zapisz lub zamknij redagowaną wiadomość.");
      return;
    }
    composeKey.current += 1;
    setCompose({ key: composeKey.current, init });
  };

  const openThread = async (t: ThreadSummary) => {
    if (compose) {
      showToast("Najpierw zapisz lub zamknij redagowaną wiadomość.");
      return;
    }
    if (folder === "drafts") {
      setOpenId(t.id);
      setDetail(null);
      try {
        const d = await api.openDraft(t.id);
        startCompose({
          mode: "draft", to: d.to, cc: d.cc, bcc: d.bcc, subject: d.subject, html: d.html,
          threadId: t.id, inReplyTo: d.inReplyTo || null, references: d.references || null, draftId: d.draftId,
        });
      } catch (e) {
        showToast(errText(e));
      }
      return;
    }
    await loadDetail(t.id);
  };

  useEffect(() => {
    if (!email) return;
    api.getSignature().then(setSignature).catch(() => setSignature(""));
    try { if (localStorage.getItem(BACKUP_KEY)) setRestore(localStorage.getItem(BACKUP_KEY)); } catch { /* ignorujemy */ }
  }, [email]);

  const restoreBackup = () => {
    try {
      const b = JSON.parse(restore ?? "") as { init: ComposeInit; to: string; cc: string; bcc: string; subject: string; html: string };
      startCompose({ ...b.init, to: b.to, cc: b.cc, bcc: b.bcc, subject: b.subject, html: b.html });
    } catch { showToast("Nie udało się odczytać kopii wiadomości."); }
    try { localStorage.removeItem(BACKUP_KEY); } catch { /* ignorujemy */ }
    setRestore(null);
  };
  const dropBackup = () => {
    try { localStorage.removeItem(BACKUP_KEY); } catch { /* ignorujemy */ }
    setRestore(null);
  };

  const reply = (all: boolean) => {
    if (!detail || !email) return;
    const init = buildReply(detail.messages, detail.subject, email, all, signature, detail.id);
    if (init) startCompose(init);
  };
  const forward = () => {
    if (!detail || !email) return;
    const init = buildForward(detail.messages, detail.subject, email, signature, fmtFull);
    if (init) startCompose(init);
  };

  const runAction = async (action: ThreadAction) => {
    if (!detail) return;
    const id = detail.id;
    try {
      await api.threadAction(id, action);
    } catch (e) {
      showToast(errText(e));
      return;
    }
    const inverse = INVERSE[action];
    if (inverse) {
      setThreads((prev) => prev.filter((t) => t.id !== id));
      setOpenId(null);
      setDetail(null);
      showToast(ACTION_DONE[action] ?? "Gotowe.", () => {
        api.threadAction(id, inverse)
          .then(() => { setToast(null); void loadList(folder, null, false); })
          .catch((e) => showToast("Nie udało się cofnąć: " + errText(e)));
      });
      return;
    }
    const patch = {
      unread: action === "unread" ? true : action === "read" ? false : undefined,
      starred: action === "star" ? true : action === "unstar" ? false : undefined,
    };
    setDetail((d) => (d ? {
      ...d, unread: patch.unread ?? d.unread, starred: patch.starred ?? d.starred,
    } : d));
    setThreads((prev) => prev.map((t) => (t.id === id ? {
      ...t, unread: patch.unread ?? t.unread, starred: patch.starred ?? t.starred,
    } : t)));
  };

  const setSafe = async (safe: boolean) => {
    if (!detail) return;
    const id = detail.id;
    try {
      await api.setThreadSafe(id, safe);
    } catch (e) {
      showToast(errText(e));
      return;
    }
    setDetail((d) => (d && d.id === id ? { ...d, dismissed: safe } : d));
    setThreads((prev) =>
      folder === "suspicious" && safe
        ? prev.filter((t) => t.id !== id)
        : prev.map((t) => (t.id === id ? { ...t, dismissed: safe } : t)));
    showToast(safe ? "Oznaczono jako bezpieczne." : "Cofnięto oznaczenie jako bezpieczne.");
  };

  const senderChanged = async (moveToSpam: boolean) => {
    setSenderDlg(null);
    const id = detail?.id ?? null;
    if (moveToSpam && id) await runAction("spam");
    else if (id) void loadDetail(id);
    void loadList(folder, null, false);
  };

  const onSent = (info: { threadId: string }) => {
    setCompose(null);
    showToast("Wiadomość wysłana.");
    void loadList(folder, null, false);
    if (info.threadId && info.threadId === openId) void loadDetail(info.threadId);
  };
  const onComposeClosed = (draftSaved: boolean) => {
    setCompose(null);
    if (draftSaved) showToast("Wersja robocza zapisana.");
    if (folder === "drafts" || draftSaved) void loadList(folder, null, false);
  };

  const saveClient = async (id: string, secret: string) => {
    try { await api.saveClient(id, secret); setAuthError(null); await refreshAuth(); }
    catch (e) { setAuthError(errText(e)); }
  };
  const signIn = async () => {
    setSigningIn(true);
    setAuthError(null);
    try { await api.signIn(); await refreshAuth(); }
    catch (e) { setAuthError(errText(e)); }
    finally { setSigningIn(false); }
  };
  const clearClient = async () => {
    try { await api.clearClient(); setAuthError(null); await refreshAuth(); }
    catch (e) { setAuthError(errText(e)); }
  };
  const signOut = async () => {
    try {
      await api.signOut();
      setThreads([]); setDetail(null); setOpenId(null);
      await refreshAuth();
    } catch (e) { setAuthError(errText(e)); }
  };

  const toggleMsg = (id: string) =>
    setCollapsed((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const toggleAuto = (on: boolean) => { setAutoUpdate(on); save(LS.auto, on ? "1" : "0"); };

  const labelName = (id: string) => stats.labels.find((l) => l.id === id)?.name ?? "";
  const folderLabel = folder.startsWith("label:")
    ? labelName(folder.slice(6)) || "Etykieta"
    : (FOLDERS.find((f) => f.id === folder)?.label ?? "");

  const addLabel = async () => {
    const name = (newLabel ?? "").trim();
    if (!name) { setNewLabel(null); return; }
    try {
      const l = await api.createLabel(name);
      setStats((s) => ({ ...s, labels: [...s.labels, l].sort((a, b) => a.name.localeCompare(b.name)) }));
      setNewLabel(null);
      showToast(`Utworzono etykietę „${l.name}”.`);
    } catch (e) { showToast(errText(e)); }
  };
  const toggleLabel = async (id: string, on: boolean) => {
    if (!detail) return;
    const tid = detail.id;
    try {
      await api.setThreadLabel(tid, id, on);
      setDetail((d) => (d && d.id === tid
        ? { ...d, labels: on ? [...(d.labels ?? []), id] : (d.labels ?? []).filter((x) => x !== id) } : d));
      void pollOnce();
      if (folder === `label:${id}` && !on) setThreads((prev) => prev.filter((t) => t.id !== tid));
    } catch (e) { showToast(errText(e)); }
  };

  const clearSearch = () => {
    searchRef.current = null;
    setSearchTerm(null);
    setQuery("");
  };
  const pickFolder = (id: FolderId) => {
    const wasSearching = searchRef.current !== null;
    clearSearch();
    if (id === folder) {
      if (wasSearching) void loadList(id, null, false);
    } else {
      setFolder(id);
    }
  };
  const doSearch = (e: React.FormEvent) => {
    e.preventDefault();
    const q = query.trim();
    if (!q) {
      if (searchRef.current !== null) { clearSearch(); void loadList(folder, null, false); }
      return;
    }
    searchRef.current = q;
    setSearchTerm(q);
    setOpenId(null);
    setDetail(null);
    void loadList(folder, null, false);
  };

  // Synchronizacja przyrostowa (tylko gdy tryb offline jest włączony; w przeciwnym razie polecenie nic nie robi).
  const live = useRef({ openId: null as string | null, count: 0, composing: false });
  live.current = { openId, count: threads.length, composing: Boolean(compose) };
  const folderRef = useRef(folder);
  folderRef.current = folder;
  const settingsRef = useRef({ notify: notifyOn, content: notifyContent });
  settingsRef.current = { notify: notifyOn, content: notifyContent };
  const pollOnce = useCallback(async () => {
    try {
      const r = await api.pollMail(settingsRef.current.notify, settingsRef.current.content);
      setStats({ inbox: r.inboxUnread, spam: r.spamUnread, labels: r.labels });
      document.title = r.inboxUnread > 0 ? `(${r.inboxUnread}) RustMail` : "RustMail";
    } catch { /* brak sieci – spróbujemy za minutę */ }
  }, []);
  useEffect(() => {
    if (!email) return;
    void pollOnce();
    const t = window.setInterval(() => void pollOnce(), 60_000);
    return () => window.clearInterval(t);
  }, [email, pollOnce]);
  const syncing = useRef(false);
  const syncOnce = useCallback(async () => {
    if (syncing.current) return;
    syncing.current = true;
    try {
      const r = await api.syncNow();
      if (!r.active || r.changed.length === 0) return;
      const l = live.current;
      if (l.count <= 20 && !l.composing) void loadList(folderRef.current, null, false);
      if (l.openId && !l.composing && r.changed.includes(l.openId)) void loadDetail(l.openId);
    } catch { /* brak sieci – spróbujemy za minutę */ } finally {
      syncing.current = false;
    }
  }, [loadList, loadDetail]);
  useEffect(() => {
    if (!email) return;
    const t = window.setInterval(() => void syncOnce(), 60_000);
    return () => window.clearInterval(t);
  }, [email, syncOnce]);

  return (
    <div className="app">
      <UpdateBanner u={updater} />
      <div className="panes" style={{ gridTemplateColumns: `${left.width}px 6px ${mid.width}px 6px 1fr` }}>
        <aside className="sidebar" aria-label="Foldery">
          <div className="brand">
            <svg viewBox="0 0 24 24" className="logo" aria-hidden="true">
              <rect x="2" y="4" width="20" height="16" rx="4" />
              <path d="M4 8l8 6 8-6" />
            </svg>
            <span>RustMail</span>
          </div>
          <button className="primary" disabled={!email || needsReauth}
            title={needsReauth ? "Zaloguj się ponownie, aby wysyłać wiadomości" : undefined}
            onClick={() => startCompose(buildNew(signature))}>Nowa wiadomość</button>
          <nav>
            {FOLDERS.map((f) => (
              <button key={f.id} className={"folder" + (f.id === folder ? " active" : "")}
                onClick={() => pickFolder(f.id)} aria-current={f.id === folder && !searchTerm}>
                <Icon d={f.icon} />
                <span>{f.label}</span>
                {f.id === "inbox" && stats.inbox > 0 && <span className="badge">{stats.inbox}</span>}
                {f.id === "spam" && stats.spam > 0 && <span className="badge muted">{stats.spam}</span>}
              </button>
            ))}
            {email && (
              <>
                <div className="nav-title">Etykiety</div>
                {stats.labels.map((l) => (
                  <button key={l.id} className={"folder" + (folder === `label:${l.id}` ? " active" : "")}
                    onClick={() => pickFolder(`label:${l.id}`)} aria-current={folder === `label:${l.id}` && !searchTerm}>
                    <Icon d="M4 4h9l7 8-7 8H4z" />
                    <span>{l.name}</span>
                    {l.unread > 0 && <span className="badge">{l.unread}</span>}
                  </button>
                ))}
                {newLabel === null ? (
                  <button className="folder dim" disabled={needsReauth} onClick={() => setNewLabel("")}>+ Nowa etykieta</button>
                ) : (
                  <form className="newlabel" onSubmit={(e) => { e.preventDefault(); void addLabel(); }}>
                    <input autoFocus value={newLabel} maxLength={100} aria-label="Nazwa nowej etykiety"
                      onChange={(e) => setNewLabel(e.target.value)} onBlur={() => { if (!newLabel.trim()) setNewLabel(null); }} />
                    <button className="small" type="submit">Dodaj</button>
                  </form>
                )}
              </>
            )}
          </nav>
          <div className="spacer" />
          <div className="theme" role="group" aria-label="Motyw">
            {(["auto", "light", "dark"] as Theme[]).map((t) => (
              <button key={t} className={theme === t ? "on" : ""} onClick={() => setTheme(t)}>
                {t === "auto" ? "Systemowy" : t === "light" ? "Jasny" : "Ciemny"}
              </button>
            ))}
          </div>
        </aside>

        <div className="handle" onPointerDown={left.onPointerDown} role="separator" aria-orientation="vertical" />

        <section className="list" aria-label="Lista konwersacji">
          <header className="list-head">
            <h2>{searchTerm ? `Wyniki: „${searchTerm}”` : folderLabel}</h2>
            {email && (
              <button className="small" disabled={loadingList} onClick={() => void loadList(folder, null, false)}>
                {loadingList ? "Wczytywanie…" : "Odśwież"}
              </button>
            )}
          </header>
          {email && (
            <form className="search" role="search" onSubmit={doSearch}>
              <input type="search" value={query} maxLength={500} placeholder="Szukaj w poczcie (np. from:ala faktura)"
                aria-label="Szukaj w poczcie" onChange={(e) => setQuery(e.target.value)} />
              <button className="small" type="submit">Szukaj</button>
              {searchTerm && <button className="small" type="button" onClick={() => { clearSearch(); void loadList(folder, null, false); }}>Wyczyść</button>}
            </form>
          )}
          {email && listOffline && (
            <p className="offline-note" role="status">
              Brak połączenia z Gmail – pokazuję wiadomości zapisane na tym komputerze (mogą być nieaktualne).
              {searchTerm ? " Wyszukiwanie obejmuje tylko zapisane wątki." : ""}
            </p>
          )}
          {!email ? (
            <div className="empty">
              <h3>Nie połączono konta</h3>
              <p>Zaloguj się po prawej stronie, aby zobaczyć pocztę.</p>
            </div>
          ) : (
            <>
              {folder === "suspicious" && (
                <p className="hint pad">
                  Ten folder jest tylko widokiem w RustMail: pokazuje wiadomości z ostatnich 100 wątków skrzynki odbiorczej, które
                  mają ostrzeżenie. Nic nie jest przenoszone ani usuwane w Gmailu. Ocena z nagłówków jest wstępna.
                </p>
              )}
              {listError && <p className="error pad" role="alert">{listError}</p>}
              {!loadingList && !listError && threads.length === 0 && (
                <div className="empty">
                  <h3>Ten folder jest pusty</h3>
                  {folder === "suspicious" && <p>Wśród ostatnich 100 wątków skrzynki odbiorczej nie znaleziono wiadomości z ostrzeżeniem.</p>}
                </div>
              )}
              <ul>
                {threads.map((t) => (
                  <li key={t.id}>
                    <button className={"row" + (t.id === openId ? " active" : "") + (t.unread ? " unread" : "")}
                      onClick={() => void openThread(t)}>
                      <span className="who">{t.participants}{t.count > 1 ? ` (${t.count})` : ""}</span>
                      <span className="when">
                        {t.starred && <Icon d={STAR} />} {fmtDate(t.lastDateMs)}
                      </span>
                      <span className="subj">
                        {!t.dismissed && t.risk.level !== "none" && (
                          <span className={"risk-chip " + t.risk.level} title={t.risk.reasons.join("\n")}>
                            {t.risk.level === "high" ? "Podejrzane" : "Uwaga"}
                          </span>
                        )}
                        {(t.labels ?? []).map((id) => {
                          const n = labelName(id);
                          return n ? <span key={id} className="label-chip">{n}</span> : null;
                        })}
                        {t.subject}
                      </span>
                      <span className="snip">{t.snippet}</span>
                    </button>
                  </li>
                ))}
              </ul>
              {nextToken && (
                <button className="more" disabled={loadingList} onClick={() => void loadList(folder, nextToken, true)}>
                  {loadingList ? "Wczytywanie…" : "Wczytaj więcej"}
                </button>
              )}
            </>
          )}
        </section>

        <div className="handle" onPointerDown={mid.onPointerDown} role="separator" aria-orientation="vertical" />

        <main className="reader" aria-label="Konwersacja">
          {email && needsReauth && (
            <div className="banner warn" role="alert">
              <span>Aby wysyłać wiadomości i zarządzać pocztą, zaloguj się ponownie i zaakceptuj dostęp do poczty.</span>
              <button className="small solid" disabled={signingIn} onClick={() => void signIn()}>
                {signingIn ? "Czekam na przeglądarkę…" : "Zaloguj ponownie"}
              </button>
            </div>
          )}
          {email && restore && !compose && (
            <div className="banner" role="status">
              <span>Znaleziono niezapisaną wiadomość z poprzedniej sesji.</span>
              <button className="small solid" onClick={restoreBackup}>Przywróć</button>
              <button className="small" onClick={dropBackup}>Usuń</button>
            </div>
          )}
          {email && detail?.offline && (
            <div className="banner" role="status">
              <span>Ta konwersacja pochodzi z pamięci offline – nie masz połączenia z Gmail. Odpowiadanie i akcje wymagają sieci.</span>
            </div>
          )}
          {authError && email && <p className="error pad" role="alert">{authError}</p>}
          {email && compose ? (
            <Compose key={compose.key} init={compose.init} onSent={onSent} onClose={onComposeClosed} />
          ) : !auth ? (
            <div className="empty"><p>Wczytywanie…</p></div>
          ) : !email ? (
            <AuthPanel auth={auth} error={authError} busy={signingIn}
              onSave={(id, s) => void saveClient(id, s)} onSignIn={() => void signIn()} onClear={() => void clearClient()} />
          ) : !openId ? (
            <div className="empty"><p>Wybierz konwersację z listy.</p></div>
          ) : detailLoading ? (
            <div className="empty"><p>Wczytywanie konwersacji…</p></div>
          ) : detailError ? (
            <div className="empty"><p className="error" role="alert">{detailError}</p></div>
          ) : detail ? (
            <>
              <header className="reader-head">
                <h2>{detail.subject}</h2>
                <div className="tools actions" role="toolbar" aria-label="Akcje na konwersacji">
                  <button className="primary" disabled={needsReauth} onClick={() => reply(false)}>Odpowiedz</button>
                  <button disabled={needsReauth} onClick={() => reply(true)}>Odpowiedz wszystkim</button>
                  <button disabled={needsReauth} onClick={forward}>Przekaż dalej</button>
                  <span className="sep" />
                  {detail.inInbox && <button disabled={needsReauth} onClick={() => void runAction("archive")}>Archiwizuj</button>}
                  {!detail.inInbox && !detail.inTrash && !detail.inSpam && (
                    <button disabled={needsReauth} onClick={() => void runAction("unarchive")}>Do skrzynki</button>
                  )}
                  {detail.inSpam
                    ? <button disabled={needsReauth} onClick={() => void runAction("notspam")}>To nie spam</button>
                    : <button disabled={needsReauth} onClick={() => void runAction("spam")}>Spam</button>}
                  {detail.inTrash
                    ? <button disabled={needsReauth} onClick={() => void runAction("untrash")}>Przywróć z kosza</button>
                    : <button disabled={needsReauth} onClick={() => void runAction("trash")}>Usuń</button>}
                  <button disabled={needsReauth} onClick={() => void runAction(detail.unread ? "read" : "unread")}>
                    {detail.unread ? "Oznacz jako przeczytaną" : "Oznacz jako nieprzeczytaną"}
                  </button>
                  {stats.labels.length > 0 && (
                    <details className="label-menu">
                      <summary>Etykiety</summary>
                      <div className="label-pop">
                        {stats.labels.map((l) => (
                          <label key={l.id} className="check">
                            <input type="checkbox" disabled={needsReauth || Boolean(detail.offline)}
                              checked={(detail.labels ?? []).includes(l.id)}
                              onChange={(e) => void toggleLabel(l.id, e.target.checked)} />
                            {l.name}
                          </label>
                        ))}
                      </div>
                    </details>
                  )}
                  <button disabled={needsReauth} aria-pressed={detail.starred} onClick={() => void runAction(detail.starred ? "unstar" : "star")}>
                    {detail.starred ? "★ Usuń gwiazdkę" : "☆ Gwiazdka"}
                  </button>
                  {(() => {
                    const t = replyTarget(detail.messages);
                    return t && !t.mine ? (
                      <button onClick={() => setSenderDlg({ email: t.fromEmail })}>Nadawca…</button>
                    ) : null;
                  })()}
                  <span className="sep" />
                  <button onClick={() => setCollapsed(new Set())}>Rozwiń wszystkie</button>
                  <button onClick={() => setCollapsed(new Set(detail.messages.map((m) => m.id)))}>Zwiń wszystkie</button>
                </div>
              </header>
              <RiskBanner messages={detail.messages} dismissed={detail.dismissed} onSafe={(s) => void setSafe(s)} />
              <div className="thread">
                {detail.messages.map((m) => {
                  const folded = collapsed.has(m.id);
                  return (
                    <article key={m.id} className={"bubble " + (m.mine ? "sent" : "received") + (m.html ? " wide" : "") + (!detail.dismissed && m.risk.level !== "none" ? " risk-" + m.risk.level : "")}>
                      <button className="bubble-head" onClick={() => toggleMsg(m.id)} aria-expanded={!folded}>
                        <strong title={m.fromEmail}>{m.from}</strong>
                        <time>{fmtFull(m.dateMs)}</time>
                      </button>
                      {folded ? (
                        <p className="fold">{splitQuoted(m.body).main.slice(0, 120)}</p>
                      ) : (
                        <>
                          <MessageBody m={m} strict={m.risk.level === "high" && !detail.dismissed} />
                          <Attachments m={m} strict={m.risk.level === "high" && !detail.dismissed} />
                        </>
                      )}
                    </article>
                  );
                })}
              </div>
            </>
          ) : null}
        </main>
      </div>

      <footer className="status">
        <span>{info ? `${info.name} ${info.version}${info.appimage ? " · AppImage" : ""}` : "Tryb podglądu w przeglądarce"}</span>
        <span className="pill">{email ?? "brak konta"}</span>
        {email && <button className="linklike plain" onClick={() => void signOut()}>Wyloguj</button>}
        {email && <button className="linklike plain" onClick={() => setSettingsOpen(true)}>Ustawienia</button>}
        <span className="grow" />
        <label className="check">
          <input type="checkbox" checked={autoUpdate} onChange={(e) => toggleAuto(e.target.checked)} />
          Sprawdzaj aktualizacje
        </label>
        <button className="linklike" onClick={() => void updater.check(true)}>Sprawdź teraz</button>
        {updater.state?.hasPrevious && (
          confirmRollback ? (
            <button className="linklike warn" onClick={() => { setConfirmRollback(false); void updater.rollback(); }}>
              Na pewno? Kliknij ponownie
            </button>
          ) : (
            <button className="linklike" onClick={() => setConfirmRollback(true)}>Przywróć poprzednią wersję</button>
          )
        )}
      </footer>

      {toast && (
        <div className="toast" role="status" key={toast.key}>
          <span>{toast.text}</span>
          {toast.undo && <button className="small" onClick={toast.undo}>Cofnij</button>}
          <button className="small" aria-label="Zamknij powiadomienie" onClick={() => setToast(null)}>✕</button>
        </div>
      )}
      {senderDlg && (
        <SenderLists email={senderDlg.email} onClose={() => setSenderDlg(null)} onChanged={(s) => void senderChanged(s)} />
      )}
      {settingsOpen && (
        <SettingsDialog notify={notifyOn} content={notifyContent}
          onNotify={(on) => { setNotifyOn(on); save(LS.notify, on ? "1" : "0"); }}
          onContent={(on) => { setNotifyContent(on); save(LS.content, on ? "1" : "0"); }}
          onOpen={(w) => { setSettingsOpen(false); if (w === "signature") setSigOpen(true); else if (w === "senders") setSenderDlg({ email: null }); else setCacheOpen(true); }}
          onClose={() => setSettingsOpen(false)} />
      )}
      {cacheOpen && <CacheDialog onClose={() => setCacheOpen(false)} onChanged={() => void syncOnce()} />}
      {sigOpen && (
        <SignatureDialog initial={signature} onClose={() => setSigOpen(false)}
          onSaved={(t) => { setSignature(t); setSigOpen(false); showToast("Podpis zapisany."); }} />
      )}
    </div>
  );
}
