import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { Attachments } from "./Attachments";
import { AuthPanel } from "./AuthPanel";
import { CacheDialog } from "./CacheDialog";
import { SettingsDialog } from "./SettingsDialog";
import { AccountSwitcher } from "./AccountSwitcher";
import { OutboxDialog } from "./OutboxDialog";
import { commandFor, isTypingTarget, SHORTCUT_HELP } from "./shortcuts";
import { BACKUP_KEY, Compose } from "./Compose";
import { MessageBody } from "./MessageBody";
import { RiskBanner } from "./RiskBanner";
import { splitQuoted } from "./quoteTrim";
import { SenderLists } from "./SenderLists";
import { SignatureDialog } from "./SignatureDialog";
import { SpamDialog } from "./SpamDialog";
import { ImportDialog } from "./ImportDialog";
import { ImapLogin } from "./ImapLogin";
import { ImapListPane, ImapReader, ImapSidebarNav, toView, useImapInboxes, useImapView } from "./ImapPanes";
import { buildForward, buildNew, buildReply, replyTarget, type ComposeInit } from "./replyLogic";
import { FOLDERS, type FolderId } from "./folders";
import { UpdateBanner } from "./UpdateBanner";
import { useUpdater } from "./useUpdater";
import { fmtDate, fmtFull } from "./util";
import type { Account, AppInfo, AuthStatus, ImapAccount, ImapMsg, ImapRow, LabelInfo, OutboxItem, ThreadAction, ThreadDetail, ThreadSummary } from "./types";
import { THEMES, isThemeId, resolveTheme, type ThemeId } from "./themes";
import Titlebar from "./Titlebar";
import "./themes.css";
import "./styles.css";
import "./compose.css";

type Theme = ThemeId;
const LS = { theme: "rm.theme", left: "rm.left", mid: "rm.mid", auto: "rm.autoupdate", notify: "rm.notify", content: "rm.notifycontent", view: "rm.view" };
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
interface ComposeState { key: number; init: ComposeInit; from: string }
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
  const [theme, setTheme] = useState<Theme>(() => { const v = load(LS.theme, "auto"); return isThemeId(v) ? v : "auto"; });
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
  const [outbox, setOutbox] = useState<OutboxItem[]>([]);
  const [outboxOpen, setOutboxOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [spamOpen, setSpamOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [imapLoginOpen, setImapLoginOpen] = useState(false);
  const [imapAccs, setImapAccs] = useState<ImapAccount[]>([]);
  const [imapLoaded, setImapLoaded] = useState(false);
  const [view, setView] = useState<string>(() => load(LS.view, "gmail"));
  const [imapUid, setImapUid] = useState<number | null>(null);
  const [imapOpenItem, setImapOpenItem] = useState<{ account: string; uid: number } | null>(null);
  const imapAccount = view.startsWith("imap:") ? view.slice(5) : null;
  const imap = useImapView(imapAccount);
  const inboxes = useImapInboxes(imapAccs.map((a) => a.email), view === "all");
  const changeView = (v: string) => { setView(v); save(LS.view, v); setImapUid(null); setImapOpenItem(null); };
  useEffect(() => {
    api.imapAccounts().then(setImapAccs).catch(() => setImapAccs([])).finally(() => setImapLoaded(true));
  }, []);
  useEffect(() => {
    if (!imapLoaded) return;
    const okAcc = imapAccount === null || imapAccs.some((a) => a.email === imapAccount);
    const okAll = view !== "all" || imapAccs.length > 0;
    if (!okAcc || !okAll) changeView("gmail");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imapLoaded, imapAccs]);
  useEffect(() => { setImapUid(null); }, [imap.folder]);
  useEffect(() => {
    if (view !== "all") return;
    const id = window.setInterval(() => void inboxes.refresh(), 60000);
    return () => window.clearInterval(id);
  }, [view, inboxes.refresh]);
  const [flags, setFlags] = useState<Record<string, string>>({});
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
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
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const r = resolveTheme(theme, mq.matches);
      root.setAttribute("data-theme", r.id);
      root.setAttribute("data-mode", r.mode);
    };
    apply();
    if (mounted.current) save(LS.theme, theme);
    mounted.current = true;
    if (theme !== "auto") return;
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
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
      if (!append) setSelected(new Set());
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

  const startCompose = (init: ComposeInit, from?: string) => {
    if (compose) {
      showToast("Najpierw zapisz lub zamknij redagowaną wiadomość.");
      return;
    }
    composeKey.current += 1;
    setCompose({ key: composeKey.current, init, from: from ?? (imapAccount ? "imap:" + imapAccount : "gmail") });
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
  const composeFromImap = (account: string, kind: "reply" | "replyAll" | "forward", m: ImapMsg) => {
    if (compose) { showToast("Najpierw zapisz lub zamknij redagowaną wiadomość."); return; }
    const v = toView(m);
    const init = kind === "forward"
      ? buildForward([v], m.subject, account, signature, fmtFull)
      : buildReply([v], m.subject, account, kind === "replyAll", signature, "");
    if (init) startCompose(init, "imap:" + account);
  };
  const forward = () => {
    if (!detail || !email) return;
    const init = buildForward(detail.messages, detail.subject, email, signature, fmtFull);
    if (init) startCompose(init);
  };

  // Filtr spamu uczy się z decyzji użytkownika (Spam / To nie spam). Błędy ignorujemy.
  const trainFrom = (ids: string[], action: ThreadAction) => {
    if (action !== "spam" && action !== "notspam") return;
    const items = ids.flatMap((id) => {
      const t = threads.find((x) => x.id === id);
      return t ? [{ id, subject: t.subject, snippet: t.snippet, participants: t.participants }] : [];
    });
    if (items.length > 0) api.spamTrain(items, action === "spam").catch(() => {});
    setFlags((f) => { const n = { ...f }; for (const id of ids) delete n[id]; return n; });
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
    trainFrom([id], action);
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

  // ---------- Zaznaczanie wielu konwersacji i akcje hurtowe ----------
  const toggleSelected = (id: string) =>
    setSelected((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id); else n.add(id);
      return n;
    });
  const anchorId = useRef<string | null>(null);
  // Ctrl+klik dodaje/odejmuje wiersz od zaznaczenia, Shift+klik zaznacza zakres, zwykłe kliknięcie otwiera wątek.
  const onRowClick = (e: React.MouseEvent, t: ThreadSummary) => {
    if (e.ctrlKey || e.metaKey) {
      toggleSelected(t.id);
      anchorId.current = t.id;
      return;
    }
    if (e.shiftKey && anchorId.current) {
      const a = threads.findIndex((x) => x.id === anchorId.current);
      const b = threads.findIndex((x) => x.id === t.id);
      if (a >= 0 && b >= 0) {
        const [from, to] = a < b ? [a, b] : [b, a];
        setSelected(new Set(threads.slice(from, to + 1).map((x) => x.id)));
        return;
      }
    }
    anchorId.current = t.id;
    if (selected.size > 0) setSelected(new Set());
    void openThread(t);
  };
  const bulkRun = async (action: ThreadAction) => {
    const ids = [...selected].slice(0, 50);
    if (ids.length === 0) return;
    const done: string[] = [];
    let failed = 0;
    let lastError = "";
    for (const id of ids) {
      try { await api.threadAction(id, action); done.push(id); }
      catch (e) { failed += 1; lastError = errText(e); }
    }
    trainFrom(done, action);
    const inverse = INVERSE[action];
    const gone = new Set(done);
    if (inverse) {
      setThreads((prev) => prev.filter((t) => !gone.has(t.id)));
      if (openId && gone.has(openId)) { setOpenId(null); setDetail(null); }
    } else {
      const unread = action === "unread" ? true : action === "read" ? false : undefined;
      setThreads((prev) => prev.map((t) => (gone.has(t.id) && unread !== undefined ? { ...t, unread } : t)));
      setDetail((d) => (d && gone.has(d.id) && unread !== undefined ? { ...d, unread } : d));
    }
    setSelected(new Set());
    const msg = `${ACTION_DONE[action] ?? "Gotowe."} (${done.length})` + (failed ? ` Błędy: ${failed} – ${lastError}` : "");
    if (inverse && done.length > 0) {
      showToast(msg, () => {
        void (async () => {
          for (const id of done) { try { await api.threadAction(id, inverse); } catch { /* pomijamy */ } }
          setToast(null);
          void loadList(folder, null, false);
        })();
      });
    } else {
      showToast(msg);
    }
    void pollOnce();
  };

  // ---------- Skróty klawiszowe ----------
  const keyRef = useRef<(e: KeyboardEvent) => void>(() => {});
  keyRef.current = (e: KeyboardEvent) => {
    // Ctrl+A zaznacza wszystkie konwersacje na liście (poza polami tekstowymi i oknami).
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "a") {
      if (isTypingTarget(e.target as HTMLElement | null)) return;
      if (!email || compose || helpOpen || settingsOpen || cacheOpen || sigOpen || senderDlg || outboxOpen || spamOpen || importOpen || imapLoginOpen) return;
      e.preventDefault();
      setSelected(new Set(threads.map((t) => t.id)));
      return;
    }
    const cmd = commandFor(e);
    if (!cmd || isTypingTarget(e.target as HTMLElement | null)) return;
    if (cmd === "help") { setHelpOpen((v) => !v); e.preventDefault(); return; }
    if (!email || compose || helpOpen || settingsOpen || cacheOpen || sigOpen || senderDlg || outboxOpen || spamOpen || importOpen || imapLoginOpen) {
      if (cmd === "escape" && helpOpen) setHelpOpen(false);
      return;
    }
    const idx = threads.findIndex((t) => t.id === openId);
    const go = (i: number) => {
      const t = threads[i];
      if (!t) return;
      void openThread(t);
      document.querySelector(`[data-thread="${t.id}"]`)?.scrollIntoView({ block: "nearest" });
    };
    const act = (a: ThreadAction) => {
      if (needsReauth) return;
      if (selected.size > 0) void bulkRun(a); else void runAction(a);
    };
    e.preventDefault();
    switch (cmd) {
      case "next": go(idx < 0 ? 0 : Math.min(idx + 1, threads.length - 1)); break;
      case "prev": go(idx <= 0 ? 0 : idx - 1); break;
      case "select": { const id = openId ?? threads[0]?.id; if (id) toggleSelected(id); break; }
      case "archive": if (selected.size > 0 || detail?.inInbox) act("archive"); break;
      case "trash": if (selected.size > 0 || detail) act(detail?.inTrash && selected.size === 0 ? "untrash" : "trash"); break;
      case "spam": if (selected.size > 0 || detail) act(detail?.inSpam && selected.size === 0 ? "notspam" : "spam"); break;
      case "read": act("read"); break;
      case "unread": act("unread"); break;
      case "star": if (detail && !needsReauth) void runAction(detail.starred ? "unstar" : "star"); break;
      case "reply": reply(false); break;
      case "replyAll": reply(true); break;
      case "forward": forward(); break;
      case "compose": startCompose(buildNew(signature)); break;
      case "search": (document.querySelector('input[type="search"]') as HTMLInputElement | null)?.focus(); break;
      case "escape": setSelected(new Set()); break;
    }
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keyRef.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

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

  const onSent = (info: { threadId: string; warning?: string | null }) => {
    setCompose(null);
    showToast(info.warning ? info.warning : "Wiadomość wysłana.");
    if (imapAccount) imap.refresh();
    void loadList(folder, null, false);
    if (info.threadId && info.threadId === openId) void loadDetail(info.threadId);
  };
  const onQueued = () => {
    setCompose(null);
    showToast("Zapisano w kolejce – wiadomość wyśle się, gdy wróci połączenie.");
    void flushQueue(false);
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
  useEffect(() => {
    if (!email) { setAccounts([]); return; }
    api.listAccounts().then(setAccounts).catch(() => setAccounts([]));
  }, [email]);

  const switchAccount = async (target: string) => {
    if (compose) { showToast("Najpierw zapisz lub zamknij redagowaną wiadomość."); return; }
    try {
      await api.switchAccount(target);
      setThreads([]); setDetail(null); setOpenId(null); setSelected(new Set()); setOutbox([]);
      setStats({ inbox: 0, spam: 0, labels: [] });
      searchRef.current = null; setSearchTerm(null); setQuery("");
      setFolder("inbox");
      await refreshAuth();
      showToast(`Przełączono na ${target}.`);
    } catch (e) { showToast(errText(e)); }
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
  const signOutImap = async (a: string) => {
    try { await api.imapLogout(a); } catch (e) { showToast(errText(e)); return; }
    setImapAccs((p) => p.filter((x) => x.email !== a));
    if (imapAccount === a) changeView("gmail");
    showToast(`Wylogowano konto ${a}.`);
  };
  useEffect(() => { if (view === "all") pickFolder("inbox"); }, [view]); // eslint-disable-line react-hooks/exhaustive-deps

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
  const reloadRef = useRef<() => void>(() => {});
  reloadRef.current = () => void loadList(folderRef.current, null, false);
  const pollOnce = useCallback(async () => {
    try {
      const r = await api.pollMail(settingsRef.current.notify, settingsRef.current.content);
      setStats({ inbox: r.inboxUnread, spam: r.spamUnread, labels: r.labels });
      if (r.spamFlagged.length > 0) {
        setFlags((f) => { const n = { ...f }; for (const x of r.spamFlagged) n[x.id] = x.reason; return n; });
      }
      if (r.spamMoved > 0) {
        showToast(r.spamMoved === 1 ? "Filtr przeniósł 1 wiadomość do Spamu." : `Filtr przeniósł wiadomości do Spamu: ${r.spamMoved}.`);
        if (folderRef.current === "inbox") reloadRef.current();
      }
      document.title = r.inboxUnread > 0 ? `(${r.inboxUnread}) RustMail` : "RustMail";
      await flushQueue(false);
    } catch { /* brak sieci – spróbujemy za minutę */ }
  }, []);
  const flushQueue = async (manual: boolean) => {
    try {
      let items = await api.listOutbox();
      setOutbox(items);
      if (items.some((i) => i.state === "queued")) {
        const r = await api.flushOutbox();
        if (r.sent > 0) showToast(r.sent === 1 ? "Wysłano wiadomość z kolejki." : `Wysłano wiadomości z kolejki: ${r.sent}.`);
        if (r.failed > 0) showToast("Nie udało się wysłać części wiadomości z kolejki – zobacz „Do wysłania”.");
        items = await api.listOutbox();
        setOutbox(items);
      } else if (manual) {
        showToast("Kolejka jest pusta albo wymaga Twojej decyzji.");
      }
    } catch (e) { if (manual) showToast(errText(e)); }
  };
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

  const composeNode = compose ? (
    <Compose key={compose.key} init={compose.init} from={compose.from} onSent={onSent} onQueued={onQueued} onClose={onComposeClosed}
      senders={email ? [{ id: "gmail", label: email }, ...imapAccs.map((a) => ({ id: "imap:" + a.email, label: a.email }))] : undefined} />
  ) : null;
  const unified = view === "all" && folder === "inbox" && !searchTerm;
  type Item = { k: "g"; ts: number; t: ThreadSummary } | { k: "i"; ts: number; i: { account: string; row: ImapRow } };
  const merged: Item[] = [
    ...threads.map((t): Item => ({ k: "g", ts: t.lastDateMs, t })),
    ...(unified ? inboxes.items.map((i): Item => ({ k: "i", ts: i.row.dateMs, i })) : []),
  ];
  if (unified) merged.sort((a, b) => b.ts - a.ts);

  return (
    <div className="app">
      <Titlebar />
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
          {email && (
            <AccountSwitcher gmail={accounts.length ? accounts : [{ email, active: true }]} imap={imapAccs} view={view}
              label={imapAccount ?? (view === "all" ? "Wszystkie skrzynki" : email)} busy={signingIn}
              onGmail={(a) => { changeView("gmail"); if (a !== email) void switchAccount(a); }}
              onImap={(a) => changeView("imap:" + a)} onAll={() => changeView("all")}
              onAddGmail={() => void signIn()} onAddImap={() => setImapLoginOpen(true)}
              onSignOut={() => void signOut()} onSignOutImap={(a) => void signOutImap(a)} />
          )}
          <nav>
            {imapAccount ? <ImapSidebarNav imap={imap} /> : FOLDERS.map((f) => (
              <button key={f.id} className={"folder" + (f.id === folder ? " active" : "")}
                onClick={() => pickFolder(f.id)} aria-current={f.id === folder && !searchTerm}>
                <Icon d={f.icon} />
                <span>{f.label}</span>
                {f.id === "inbox" && stats.inbox > 0 && <span className="badge">{stats.inbox}</span>}
                {f.id === "spam" && stats.spam > 0 && <span className="badge muted">{stats.spam}</span>}
              </button>
            ))}
            {email && !imapAccount && (
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
          <label className="theme">
            <span>Motyw</span>
            <select value={theme} onChange={(e) => setTheme(isThemeId(e.target.value) ? e.target.value : "auto")}>
              {THEMES.map((t) => (
                <option key={t.id} value={t.id}>{t.label}</option>
              ))}
            </select>
          </label>
        </aside>

        <div className="handle" onPointerDown={left.onPointerDown} role="separator" aria-orientation="vertical" />

        {imapAccount ? (
          <>
            <ImapListPane imap={imap} openUid={imapUid} onOpen={(row) => setImapUid(row.uid)} />
            <div className="handle" onPointerDown={mid.onPointerDown} role="separator" aria-orientation="vertical" />
            {compose ? <main className="reader" aria-label="Nowa wiadomość">{composeNode}</main> : <ImapReader account={imapAccount} folder={imap.folder} role={imap.folders.find((f) => f.name === imap.folder)?.role ?? "other"} uid={imapUid}
              onCompose={(k, m) => composeFromImap(imapAccount, k, m)}
              onChanged={(uid, p) => { if (p === "removed") { imap.remove(uid); setImapUid(null); } else imap.patch(uid, p); }} />}
          </>
        ) : (
        <>
        <section className="list" aria-label="Lista konwersacji">
          <header className="list-head">
            <h2>{searchTerm ? `Wyniki: „${searchTerm}”` : unified ? "Wszystkie skrzynki" : folderLabel}</h2>
            {email && (
              <button className="small" disabled={loadingList} onClick={() => { void loadList(folder, null, false); if (view === "all") void inboxes.refresh(); }}>
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
              {unified && inboxes.error && <p className="error pad" role="alert">{inboxes.error}</p>}
              {!loadingList && !listError && threads.length === 0 && (
                <div className="empty">
                  <h3>Ten folder jest pusty</h3>
                  {folder === "suspicious" && <p>Wśród ostatnich 100 wątków skrzynki odbiorczej nie znaleziono wiadomości z ostrzeżeniem.</p>}
                </div>
              )}
              {selected.size > 0 && (
                <div className="bulkbar" role="toolbar" aria-label="Akcje na zaznaczonych">
                  <strong>Zaznaczono: {selected.size}</strong>
                  <button className="small" onClick={() => setSelected(new Set(threads.map((t) => t.id)))}>Wszystkie</button>
                  {folder !== "archive" && folder !== "trash" && folder !== "spam" && (
                    <button className="small" disabled={needsReauth} onClick={() => void bulkRun("archive")}>Archiwizuj</button>
                  )}
                  {folder === "trash"
                    ? <button className="small" disabled={needsReauth} onClick={() => void bulkRun("untrash")}>Przywróć</button>
                    : <button className="small" disabled={needsReauth} onClick={() => void bulkRun("trash")}>Usuń</button>}
                  {folder === "spam"
                    ? <button className="small" disabled={needsReauth} onClick={() => void bulkRun("notspam")}>To nie spam</button>
                    : <button className="small" disabled={needsReauth} onClick={() => void bulkRun("spam")}>Spam</button>}
                  <button className="small" disabled={needsReauth} onClick={() => void bulkRun("read")}>Przeczytane</button>
                  <button className="small" disabled={needsReauth} onClick={() => void bulkRun("unread")}>Nieprzeczytane</button>
                  <button className="small" onClick={() => setSelected(new Set())}>Odznacz</button>
                </div>
              )}
              <ul>
                {merged.map((m) => {
                  if (m.k === "i") {
                    const it = m.i;
                    const act = imapOpenItem?.account === it.account && imapOpenItem.uid === it.row.uid;
                    return (
                      <li key={"i" + it.account + it.row.uid}>
                        <button className={"row" + (act ? " active" : "") + (it.row.unread ? " unread" : "")}
                          onClick={() => { setImapOpenItem({ account: it.account, uid: it.row.uid }); }}>
                          <span className="who">{it.row.from}</span>
                          <span className="when">{it.row.starred ? "★ " : ""}{fmtDate(it.row.dateMs)}</span>
                          <span className="subj"><span className="label-chip">{it.account}</span>{it.row.subject || "(bez tematu)"}</span>
                        </button>
                      </li>
                    );
                  }
                  const t = m.t;
                  return (
                  <li key={t.id} className={selected.has(t.id) ? "picked" : ""}>
                    <button data-thread={t.id}
                      className={"row" + (t.id === openId && !imapOpenItem ? " active" : "") + (t.unread ? " unread" : "")}
                      aria-pressed={selected.has(t.id)}
                      onClick={(e) => { setImapOpenItem(null); onRowClick(e, t); }}>
                      <span className="who">{t.participants}{t.count > 1 ? ` (${t.count})` : ""}</span>
                      <span className="when">
                        {t.starred && <Icon d={STAR} />} {fmtDate(t.lastDateMs)}
                      </span>
                      <span className="subj">
                        {flags[t.id] && (
                          <span className="risk-chip warn" title={flags[t.id]}>Możliwy spam</span>
                        )}
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
                  );
                })}
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

        {unified && imapOpenItem && !compose ? (
          <ImapReader account={imapOpenItem.account} folder="INBOX" role="inbox" uid={imapOpenItem.uid}
            onCompose={(k, m) => composeFromImap(imapOpenItem.account, k, m)}
            onChanged={(uid, p) => { if (p === "removed") { inboxes.remove(imapOpenItem.account, uid); setImapOpenItem(null); } else inboxes.patch(imapOpenItem.account, uid, p); }} />
        ) : (
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
            composeNode
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
        )}
        </>
        )}
      </div>

      <footer className="status">
        <span>{info ? `${info.name} ${info.version}${info.appimage ? " · AppImage" : ""}` : "Tryb podglądu w przeglądarce"}</span>
        <span className="pill">{email ?? "brak konta"}</span>
        {email && outbox.length > 0 && (
          <button className="linklike plain" onClick={() => setOutboxOpen(true)}>Do wysłania ({outbox.length})</button>
        )}
        {email && <button className="linklike plain" onClick={() => setImportOpen(true)}>Import poczty</button>}
        {email && <button className="linklike plain" onClick={() => setSettingsOpen(true)}>Ustawienia</button>}
        <span className="grow" />
        <button className="linklike" onClick={() => void updater.check(true)}>Sprawdź aktualizację</button>
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
      {imapLoginOpen && (
        <ImapLogin onClose={() => setImapLoginOpen(false)}
          onAdded={(a) => {
            setImapAccs((p) => [...p.filter((x) => x.email !== a.email), a]);
            setImapLoginOpen(false); changeView("imap:" + a.email); showToast(`Dodano konto ${a.email}.`);
          }} />
      )}
      {importOpen && (
        <ImportDialog onClose={() => setImportOpen(false)} onChanged={() => void pollOnce()} />
      )}
      {spamOpen && (
        <SpamDialog onClose={() => setSpamOpen(false)} onToast={(t) => showToast(t)}
          onChanged={() => { reloadRef.current(); void pollOnce(); }} />
      )}
      {helpOpen && (
        <div className="modal-back" role="dialog" aria-modal="true" aria-label="Skróty klawiszowe">
          <div className="modal">
            <h3>Skróty klawiszowe</h3>
            <table className="keys">
              <tbody>
                {SHORTCUT_HELP.map((k) => (
                  <tr key={k.keys}><th scope="row"><kbd>{k.keys}</kbd></th><td>{k.what}</td></tr>
                ))}
              </tbody>
            </table>
            <p className="hint">Skróty nie działają podczas pisania w polu tekstowym ani w otwartym oknie.</p>
            <div className="modal-actions"><button className="primary" onClick={() => setHelpOpen(false)}>Zamknij</button></div>
          </div>
        </div>
      )}
      {outboxOpen && (
        <OutboxDialog items={outbox} onClose={() => setOutboxOpen(false)} onSendNow={() => void flushQueue(true)}
          onRetry={async (id) => { try { await api.retryOutbox(id); } catch (e) { showToast(errText(e)); } await flushQueue(true); }}
          onDelete={async (id) => { try { await api.deleteOutbox(id); } catch (e) { showToast(errText(e)); } await flushQueue(false); }} />
      )}
      {settingsOpen && (
        <SettingsDialog notify={notifyOn} content={notifyContent}
          autoUpdate={autoUpdate} onAutoUpdate={toggleAuto} hasPrevious={!!updater.state?.hasPrevious}
          onRollback={() => void updater.rollback()} onCheckUpdate={() => void updater.check(true)}
          onNotify={(on) => { setNotifyOn(on); save(LS.notify, on ? "1" : "0"); }}
          onContent={(on) => { setNotifyContent(on); save(LS.content, on ? "1" : "0"); }}
          onOpen={(w) => { setSettingsOpen(false); if (w === "signature") setSigOpen(true); else if (w === "senders") setSenderDlg({ email: null }); else if (w === "spam") setSpamOpen(true); else if (w === "import") setImportOpen(true); else setCacheOpen(true); }}
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
