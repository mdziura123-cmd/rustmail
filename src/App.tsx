import { useCallback, useEffect, useRef, useState } from "react";
import { prio, usePrioLevels } from "./prio";
import { gmailPrioKey, imapPrioKey, PRIOS, sortSearchResults } from "./prioSort";
import { flagsFirst, useFlagsFirst } from "./listPrefs";
import { TitleSearch } from "./TitleSearch";
import { useContextMenu, type MenuEntry } from "./ContextMenu";
import { textMenuFor } from "./textMenu";
import { buildQuery, EMPTY_FILTERS, filtersError, flagMatches, type FlagFilter, type Filters } from "./searchQuery";
import { FlagsFirstToggle, PrioFlag, PrioMenu } from "./PrioUi";
import { api, errText } from "./api";
import { Attachments } from "./Attachments";
import { AuthPanel } from "./AuthPanel";
import { CacheDialog } from "./CacheDialog";
import { SettingsDialog } from "./SettingsDialog";
import { AccountSwitcher } from "./AccountSwitcher";
import { OutboxDialog } from "./OutboxDialog";
import { ActIcon } from "./ActIcon";
import { loadSound, playSound, saveSound, type SoundId } from "./sound";
import { applyCustomKeys, commandFor, customKeys, isTypingTarget, shortcutHelp } from "./shortcuts";
import { Compose } from "./Compose";
import { MessageBody } from "./MessageBody";
import { RiskBanner } from "./RiskBanner";
import { splitQuoted } from "./quoteTrim";
import { SenderLists } from "./SenderLists";
import { SignatureDialog } from "./SignatureDialog";
import { SpamDialog } from "./SpamDialog";
import { ImportDialog } from "./ImportDialog";
import { ImapLogin } from "./ImapLogin";
import { ImapListPane, ImapReader, ImapSidebarNav, toView, useImapView } from "./ImapPanes";
import { ReceiveButton } from "./ReceiveButton";
import { buildForward, buildNew, buildReply, escapeHtml, replyTarget, type ComposeInit } from "./replyLogic";
import { sanitizeHtml } from "./sanitize";
import { FOLDERS, type FolderId } from "./folders";
import { StatusBar } from "./StatusBar";
import { useUpdater } from "./useUpdater";
import { fmtDate, fmtFull } from "./util";
import type { Account, AppInfo, AuthStatus, ImapAccount, ImapActOut, ImapMsg, ImapRow, LabelInfo, OutboxItem, SchedItem, SnoozeItem, ThreadAction, ThreadDetail, ThreadSummary } from "./types";
import { isThemeId, resolveTheme, type ThemeId } from "./themes";
import Titlebar from "./Titlebar";
import "./themes.css";
import "./styles.css";
import "./compose.css";

type Theme = ThemeId;
const LS = { theme: "rm.theme", left: "rm.left", mid: "rm.mid", auto: "rm.autoupdate", notify: "rm.notify", content: "rm.notifycontent", view: "rm.view", keys: "rm.keys", active: "rm.activeonly", bg: "rm.background" };
try { applyCustomKeys(JSON.parse(localStorage.getItem("rm.keys") ?? "null")); } catch { /* zapisane skróty są nieczytelne – zostają domyślne */ }
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
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS);
  const [filtErr, setFiltErr] = useState<string | null>(null);
  const [optsOpen, setOptsOpen] = useState(false);
  const flagFilterRef = useRef<FlagFilter>("any");
  const pollRef = useRef<() => Promise<void>>(async () => {});
  const ffPrev = useRef(false);
  const [searchTerm, setSearchTerm] = useState<string | null>(null);
  const [listOffline, setListOffline] = useState(false);
  const [cacheOpen, setCacheOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [outbox, setOutbox] = useState<OutboxItem[]>([]);
  const [outboxOpen, setOutboxOpen] = useState(false);
  const [scheduled, setScheduled] = useState<SchedItem[]>([]);
  const [snoozed, setSnoozed] = useState<SnoozeItem[]>([]);
  const [helpOpen, setHelpOpen] = useState(false);
  const [spamOpen, setSpamOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [imapLoginOpen, setImapLoginOpen] = useState(false);
  const [imapAccs, setImapAccs] = useState<ImapAccount[]>([]);
  // Książka adresów uzupełnia się sama z odebranych i wysłanych (raz na tydzień, po chwili od startu).
  const harvestReady = Boolean(auth?.email) || imapAccs.length > 0;
  useEffect(() => {
    if (!harvestReady) return;
    let last = 0;
    try { last = Number(localStorage.getItem("rm.harvest") ?? "0"); } catch { /* brak pamięci */ }
    if (Date.now() - last < 7 * 86_400_000) return;
    const t = window.setTimeout(() => {
      api.contactsHarvest().then(() => { try { localStorage.setItem("rm.harvest", String(Date.now())); } catch { /* */ } }).catch(() => {});
    }, 8000);
    return () => window.clearTimeout(t);
  }, [harvestReady]);
  const [imapTick, setImapTick] = useState(0);
  const [imapLoaded, setImapLoaded] = useState(false);
  const [view, setView] = useState<string>(() => { const v = load(LS.view, "gmail"); return v.startsWith("imap:") ? v : "gmail"; });
  const [imapUid, setImapUid] = useState<number | null>(null);
  const [imapSel, setImapSel] = useState<Set<number>>(new Set());
  const imapAnchor = useRef<number | null>(null);
  const imapAccount = view.startsWith("imap:") ? view.slice(5) : null;
  const imap = useImapView(imapAccount);
  const changeView = (v: string) => {
    setView(v); save(LS.view, v); setImapUid(null); setImapSel(new Set());
    // Nic z poprzedniego konta nie może zostać „otwarte” ani zaznaczone – skróty i przyciski działałyby na złym koncie.
    setOpenId(null); setDetail(null); setSelected(new Set());
  };
  useEffect(() => {
    api.imapAccounts().then(setImapAccs).catch(() => setImapAccs([])).finally(() => setImapLoaded(true));
  }, []);
  useEffect(() => {
    if (!imapLoaded) return;
    const okAcc = imapAccount === null || imapAccs.some((a) => a.email === imapAccount);
    if (!okAcc) changeView("gmail");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imapLoaded, imapAccs]);
  useEffect(() => { setImapUid(null); setImapSel(new Set()); }, [imap.folder]);
  const [flags, setFlags] = useState<Record<string, string>>({});
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [notifyOn, setNotifyOn] = useState(() => load(LS.notify, "0") === "1");
  // Domyślnie poczta jest sprawdzana w tle tylko dla konta, na którym pracujesz.
  const [activeOnly, setActiveOnly] = useState(() => load(LS.active, "1") === "1");
  const [notifyContent, setNotifyContent] = useState(() => load(LS.content, "1") === "1");
  // Domyślnie zamknięcie okna zostawia aplikację w zasobniku (poczta i powiadomienia działają dalej).
  const [background, setBackground] = useState(() => load(LS.bg, "1") === "1");
  useEffect(() => { void api.setBackground(background).catch(() => undefined); }, [background]);
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

  const emailRef = useRef<string | null>(null);
  const loadList = useCallback(async (f: FolderId, token: string | null, append: boolean) => {
    const my = ++listReq.current;
    setLoadingList(true);
    setListError(null);
    try {
      const q = searchRef.current;
      const page = q ? await api.searchThreads(q, token) : await api.listThreads(f, token);
      if (my !== listReq.current) return;
      const acc = emailRef.current ?? "";
      await prio.ensure(page.threads.map((t) => gmailPrioKey(acc, t.id)));
      if (my !== listReq.current) return;
      // Wyszukiwanie: najpierw wiadomości z flagą (bardzo ważne → mniej ważne), potem reszta od najnowszych.
      const shown = q ? page.threads.filter((t) => flagMatches(prio.get(gmailPrioKey(acc, t.id)), flagFilterRef.current)) : page.threads;
      const order = (l: ThreadSummary[]) => (q || flagsFirst() ? sortSearchResults(l, (t) => t.lastDateMs, (t) => prio.get(gmailPrioKey(acc, t.id))) : l);
      setThreads((prev) => order(append ? [...prev, ...shown] : shown));
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
  emailRef.current = email;
  const prioLevels = usePrioLevels();
  const ff = useFlagsFirst();
  // Zmiana flagi podczas wyszukiwania od razu przestawia wyniki.
  useEffect(() => {
    if (!email) return;
    const flagged = Boolean(searchRef.current) || ff;
    if (!flagged && !ffPrev.current) return;
    ffPrev.current = ff;
    setThreads((prev) => sortSearchResults(prev, (t) => t.lastDateMs, (t) => (flagged ? prio.get(gmailPrioKey(email, t.id)) : 0)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prioLevels, ff]);
  // Flagi wątków widocznych na liście (poza wyszukiwaniem też je pokazujemy).
  useEffect(() => {
    if (email && threads.length) void prio.ensure(threads.map((t) => gmailPrioKey(email, t.id)));
  }, [email, threads]);
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
          void pollRef.current();
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
    // Stara, niezaszyfrowana kopia w localStorage jest usuwana; nowa leży w zaszyfrowanej bazie.
    try { localStorage.removeItem("rm.compose.backup"); } catch { /* ignorujemy */ }
    api.composeBackupGet().then((b) => { if (b) setRestore(b); }).catch(() => { /* ignorujemy */ });
  }, [email]);

  const restoreBackup = () => {
    try {
      const b = JSON.parse(restore ?? "") as { init: ComposeInit; to: string; cc: string; bcc: string; subject: string; html: string };
      startCompose({ ...b.init, to: b.to, cc: b.cc, bcc: b.bcc, subject: b.subject, html: b.html });
    } catch { showToast("Nie udało się odczytać kopii wiadomości."); }
    api.composeBackupClear().catch(() => { /* ignorujemy */ });
    setRestore(null);
  };
  const dropBackup = () => {
    api.composeBackupClear().catch(() => { /* ignorujemy */ });
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
    void pollRef.current();
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
  const bulkRun = async (action: ThreadAction, only?: string[]) => {
    const ids = (only ?? [...selected]).slice(0, 50);
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
      const starred = action === "star" ? true : action === "unstar" ? false : undefined;
      setThreads((prev) => prev.map((t) => (gone.has(t.id) ? { ...t, unread: unread ?? t.unread, starred: starred ?? t.starred } : t)));
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
    // Skróty tej funkcji dotyczą wiadomości Gmaila. Na koncie IMAP (lub gdy w widoku wspólnym otwarta jest wiadomość IMAP)
    // nie robią nic, żeby nie zmienić wiadomości na innym koncie.
    if (view.startsWith("imap:")) {
      // Konto IMAP: nawigacja i zaznaczanie na liście; akcje na jednej wiadomości obsługuje otwarta wiadomość.
      if (isTypingTarget(e.target as HTMLElement | null) || compose || document.querySelector(".modal-back")) return;
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setImapSel(new Set(imap.rows.map((r) => r.uid)));
        return;
      }
      const c = commandFor(e);
      if (!c) return;
      const sel = imapSel.size > 0;
      const bulk = (a: string) => { e.preventDefault(); void imapBulk(a); };
      switch (c) {
        case "next":
        case "prev": {
          const i = imap.rows.findIndex((r) => r.uid === imapUid);
          const n = imap.rows[c === "next" ? (i < 0 ? 0 : Math.min(i + 1, imap.rows.length - 1)) : (i <= 0 ? 0 : i - 1)];
          if (n) { e.preventDefault(); setImapUid(n.uid); }
          break;
        }
        case "select": {
          const u = imapUid ?? imap.rows[0]?.uid;
          if (u !== undefined) { e.preventDefault(); setImapSel((p) => { const n = new Set(p); if (n.has(u)) n.delete(u); else n.add(u); return n; }); }
          break;
        }
        case "escape": setImapSel(new Set()); break;
        case "help": e.preventDefault(); setHelpOpen((v) => !v); break;
        case "search": e.preventDefault(); (document.querySelector('input[type="search"]') as HTMLInputElement | null)?.focus(); break;
        case "compose": e.preventDefault(); startCompose(buildNew(signature), "imap:" + imapAccount); break;
        case "trash": if (sel) bulk(imapRole === "trash" ? "inbox" : "trash"); break;
        case "spam": if (sel) bulk(imapRole === "spam" ? "inbox" : "spam"); break;
        case "archive": if (sel && imapRole === "inbox") bulk("archive"); break;
        case "read": if (sel) bulk("read"); break;
        case "unread": if (sel) bulk("unread"); break;
      }
      return;
    }
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
      case "prio0": case "prio1": case "prio2": case "prio3":
        if (detail) prio.set(gmailPrioKey(email, detail.id), Number(cmd.slice(4))).catch(() => showToast("Nie udało się zapisać flagi.")); break;
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
  const onScheduled = (atMs: number) => {
    setCompose(null);
    showToast(`Zaplanowano wysyłkę na ${fmtFull(atMs)}.`);
    void refreshLater();
  };
  const onComposeClosed = (draftSaved: boolean) => {
    setCompose(null);
    if (draftSaved) showToast("Wersja robocza zapisana.");
    if (imapAccount) { if (draftSaved || imapRole === "drafts") imap.refresh(); return; }
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
    flagFilterRef.current = "any";
    setSearchTerm(null);
    setQuery("");
    setFilters(EMPTY_FILTERS);
    setFiltErr(null);
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

  const doSearch = () => {
    const err = filtersError(filters);
    setFiltErr(err);
    if (err) return;
    if (imapAccount) {
      const full = buildQuery(query, filters, "imap");
      if (!full) return;
      setOptsOpen(false);
      imap.search(full, filters.flag);
      return;
    }
    const q = buildQuery(query, filters, "gmail");
    if (!q) {
      if (searchRef.current !== null) { clearSearch(); void loadList(folder, null, false); }
      return;
    }
    flagFilterRef.current = filters.flag;
    setOptsOpen(false);
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
  const [sound, setSound] = useState<SoundId>(loadSound);
  const soundRef = useRef(sound);
  soundRef.current = sound;
  const settingsRef = useRef({ notify: notifyOn, content: notifyContent });
  settingsRef.current = { notify: notifyOn, content: notifyContent };
  const reloadRef = useRef<() => void>(() => {});
  reloadRef.current = () => void loadList(folderRef.current, null, false);
  const bgRef = useRef({ activeOnly, view });
  bgRef.current = { activeOnly, view };
  const pollOnce = useCallback(async () => {
    if (bgRef.current.activeOnly && bgRef.current.view !== "gmail") {
      // Gmail jest nieaktywny: nie sprawdzamy poczty, tylko wysyłamy ewentualną kolejkę.
      try { await flushQueue(false, null); } catch { /* ignorujemy */ }
      return;
    }
    try {
      const r = await api.pollMail(settingsRef.current.notify, settingsRef.current.content);
      if (r.newCount > 0) playSound(soundRef.current);
      setStats({ inbox: r.inboxUnread, spam: r.spamUnread, labels: r.labels });
      if (r.spamFlagged.length > 0) {
        setFlags((f) => { const n = { ...f }; for (const x of r.spamFlagged) n[x.id] = x.reason; return n; });
      }
      if (r.spamMoved > 0) {
        showToast(r.spamMoved === 1 ? "Filtr przeniósł 1 wiadomość do Spamu." : `Filtr przeniósł wiadomości do Spamu: ${r.spamMoved}.`);
        if (folderRef.current === "inbox") reloadRef.current();
      }
      document.title = r.inboxUnread > 0 ? `(${r.inboxUnread}) RustMail` : "RustMail";
      await flushQueue(false, null);
    } catch { /* brak sieci – spróbujemy za minutę */ }
  }, []);
  pollRef.current = pollOnce;
  // Wyszukiwanie na koncie IMAP jest per folder: zmiana konta lub folderu czyści pole i opcje.
  useEffect(() => {
    if (!imapAccount) return;
    setQuery(""); setFilters(EMPTY_FILTERS); setFiltErr(null); setOptsOpen(false);
  }, [imapAccount, imap.folder]);
  // Kolejka „Do wysłania” działa osobno dla Gmaila (account = null) i dla każdego konta IMAP.
  const outboxOps = (account: string | null) => account
    ? {
        list: () => api.imapOutboxList(account),
        flush: () => api.imapOutboxFlush(account),
        retry: (id: number) => api.imapOutboxRetry(account, id),
        del: (id: number) => api.imapOutboxDelete(account, id),
      }
    : { list: api.listOutbox, flush: api.flushOutbox, retry: api.retryOutbox, del: api.deleteOutbox };
  const flushQueue = async (manual: boolean, target?: string | null) => {
    const account = target === undefined ? imapAccountRef.current : target;
    const ops = outboxOps(account);
    const current = () => account === imapAccountRef.current;
    try {
      let items = await ops.list();
      if (current()) setOutbox(items);
      if (items.some((i) => i.state === "queued")) {
        const r = await ops.flush();
        if (r.sent > 0) showToast(r.sent === 1 ? "Wysłano wiadomość z kolejki." : `Wysłano wiadomości z kolejki: ${r.sent}.`);
        if (r.failed > 0) showToast("Nie udało się wysłać części wiadomości z kolejki – zobacz „Do wysłania”.");
        items = await ops.list();
        if (current()) setOutbox(items);
      } else if (manual) {
        showToast("Kolejka jest pusta albo wymaga Twojej decyzji.");
      }
    } catch (e) { if (manual) showToast(errText(e)); }
  };
  // Zaplanowane wiadomości i odłożone konwersacje bieżącego widoku (lista w oknie „Do wysłania”).
  const refreshLater = useCallback(async () => {
    const acc = imapAccountRef.current;
    try { setScheduled(await api.scheduleList(acc)); } catch { setScheduled([]); }
    if (acc) { setSnoozed([]); return; }
    try { setSnoozed(await api.snoozeList()); } catch { setSnoozed([]); }
  }, []);
  const flushRef = useRef(flushQueue);
  flushRef.current = flushQueue;
  // Po zmianie widoku pokazujemy kolejkę właściwego konta (tylko odczyt, bez wysyłania).
  useEffect(() => {
    setOutbox([]);
    outboxOps(imapAccount).list().then((items) => { if (imapAccountRef.current === imapAccount) setOutbox(items); }).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);
  useEffect(() => {
    if (!email) return;
    void pollOnce();
    const t = window.setInterval(() => void pollOnce(), 60_000);
    return () => window.clearInterval(t);
  }, [email, pollOnce, view, activeOnly]);
  // Konta IMAP (np. WP): filtr spamu i powiadomienia, co minutę (niezależnie od Gmaila).
  const imapRef = useRef(imapAccs);
  imapRef.current = imapAccs;
  const [imapUnread, setImapUnread] = useState<Record<string, number>>({});
  const imapPollOnce = useCallback(async () => {
    for (const a of imapRef.current) {
      if (bgRef.current.activeOnly && bgRef.current.view !== "imap:" + a.email) continue;
      try { await flushQueue(false, a.email); } catch { /* ignorujemy */ }
      try {
        const r = await api.imapPoll(a.email, settingsRef.current.notify, settingsRef.current.content);
        if (r.newCount > 0) playSound(soundRef.current);
        setImapUnread((m) => (m[a.email] === r.unread ? m : { ...m, [a.email]: r.unread }));
        if (r.moved > 0) {
          showToast(r.moved === 1 ? `Filtr przeniósł 1 wiadomość z ${a.email} do Spamu.` : `Filtr przeniósł wiadomości z ${a.email} do Spamu: ${r.moved}.`);
          setImapTick((n) => n + 1);
        }
      } catch { /* brak sieci albo błąd serwera – spróbujemy za minutę */ }
    }
  }, [showToast]);
  const imapKey = imapAccs.map((a) => a.email).join("|");
  useEffect(() => {
    if (!imapKey) return;
    void imapPollOnce();
    const t = window.setInterval(() => void imapPollOnce(), 60_000);
    return () => window.clearInterval(t);
  }, [imapKey, imapPollOnce, view, activeOnly]);
  useEffect(() => { void refreshLater(); }, [view, email, refreshLater]);
  // Co 30 s: zaplanowane wiadomości, którym minął czas, trafiają do kolejki i są wysyłane;
  // odłożone konwersacje wracają do skrzynki jako nieprzeczytane.
  useEffect(() => {
    if (!email && !imapKey) return;
    const tick = async () => {
      try {
        const keys = await api.scheduleRelease();
        for (const k of keys) await flushRef.current(false, k);
        if (keys.length > 0) void refreshLater();
      } catch { /* baza lokalna niedostępna */ }
      if (!email) return;
      try {
        const due = await api.snoozeDue();
        let woke = 0;
        for (const s of due) {
          try {
            await api.threadAction(s.threadId, "unarchive");
            await api.threadAction(s.threadId, "unread");
            await api.snoozeDone(s.threadId);
            woke += 1;
            const title = s.subject || "(bez tematu)";
            showToast(`Wraca odłożona wiadomość: ${title}`);
            if (settingsRef.current.notify) void api.showNotification("Odłożona wiadomość wróciła", title).catch(() => undefined);
          } catch { /* spróbujemy przy następnym sprawdzeniu */ }
        }
        if (woke > 0) { playSound(soundRef.current); void refreshLater(); void pollRef.current(); }
      } catch { /* brak sieci albo bazy – spróbujemy za chwilę */ }
    };
    void tick();
    const t = window.setInterval(() => void tick(), 30_000);
    return () => window.clearInterval(t);
  }, [email, imapKey, refreshLater, showToast]);
  useEffect(() => {
    if (imapTick > 0 && view !== "gmail") void imap.refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imapTick]);
  // Znacznik nowych wiadomości na ikonie w zasobniku i w pasku (Gmail + konta IMAP).
  const totalUnread = stats.inbox + imapAccs.reduce((n, a) => n + (imapUnread[a.email] ?? 0), 0);
  useEffect(() => { void api.setUnread(totalUnread).catch(() => undefined); }, [totalUnread]);
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

  // ---------- Konto IMAP: zaznaczanie wielu wiadomości i akcje hurtowe ----------
  const imapRole = imap.folders.find((f) => f.name === imap.folder)?.role ?? "other";
  // ---------- Menu prawego przycisku ----------
  const gm = useContextMenu();
  const tm = useContextMenu();
  const ctxPending = useRef<{ id: string; cmd: "reply" | "replyAll" | "forward" } | null>(null);
  const [imapPending, setImapPending] = useState<{ uid: number; cmd: "reply" | "replyAll" | "forward" } | null>(null);
  useEffect(() => {
    const p = ctxPending.current;
    if (p && detail && detail.id === p.id) {
      ctxPending.current = null;
      if (p.cmd === "forward") forward(); else reply(p.cmd === "replyAll");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail]);

  // Odkładanie (tylko Gmail): konwersacja trafia do archiwum i wraca do skrzynki o wybranej porze.
  const snoozePresets = (): { label: string; ms: number }[] => {
    const now = new Date();
    const at = (d: Date, h: number) => { const x = new Date(d); x.setHours(h, 0, 0, 0); return x.getTime(); };
    const tomorrow = new Date(now); tomorrow.setDate(now.getDate() + 1);
    const monday = new Date(now); monday.setDate(now.getDate() + 1);
    while (monday.getDay() !== 1) monday.setDate(monday.getDate() + 1);
    const list = [{ label: "za 3 godziny", ms: now.getTime() + 3 * 3600_000 }];
    if (now.getHours() < 17) list.push({ label: "dziś o 18:00", ms: at(now, 18) });
    list.push({ label: "jutro o 8:00", ms: at(tomorrow, 8) }, { label: "w poniedziałek o 8:00", ms: at(monday, 8) });
    return list;
  };
  const snoozeThreads = async (ids: string[], wakeMs: number) => {
    let ok = 0;
    const done: string[] = [];
    for (const id of ids.slice(0, 50)) {
      const subject = threads.find((t) => t.id === id)?.subject ?? detail?.subject ?? "";
      try {
        await api.threadAction(id, "archive");
        await api.snoozeAdd(id, subject, wakeMs);
        ok += 1;
        done.push(id);
      } catch (e) { showToast(errText(e)); }
    }
    if (ok === 0) return;
    const gone = new Set(done);
    setThreads((prev) => prev.filter((t) => !gone.has(t.id)));
    if (openId && gone.has(openId)) { setOpenId(null); setDetail(null); }
    setSelected(new Set());
    showToast(`Odłożono (${ok}) do ${fmtFull(wakeMs)}.`);
    void refreshLater();
  };
  const wakeSnoozed = async (threadId: string) => {
    try {
      await api.threadAction(threadId, "unarchive");
      await api.threadAction(threadId, "unread");
      await api.snoozeDone(threadId);
      void refreshLater();
      void pollRef.current();
    } catch (e) { showToast(errText(e)); }
  };

  const gmailRowMenu = (t: ThreadSummary): MenuEntry[] => {
    const many = selected.has(t.id) && selected.size > 1;
    const ids = many ? [...selected] : [t.id];
    const n = many ? ` (${ids.length})` : "";
    const off = needsReauth;
    const key = (id: string) => gmailPrioKey(email ?? "", id);
    const level = many ? 0 : prio.get(key(t.id));
    const items: MenuEntry[] = [];
    if (!many) {
      const go = (cmd: "reply" | "replyAll" | "forward") => () => {
        ctxPending.current = { id: t.id, cmd };
        if (detail?.id === t.id) { setDetail({ ...detail }); } else void openThread(t);
      };
      items.push(
        { label: "Odpowiedz", icon: "reply", disabled: off, onClick: go("reply") },
        { label: "Odpowiedz wszystkim", icon: "replyAll", disabled: off, onClick: go("replyAll") },
        { label: "Przekaż dalej", icon: "forward", disabled: off, onClick: go("forward") },
        "sep",
      );
    }
    items.push(
      t.unread
        ? { label: "Oznacz jako przeczytaną" + n, icon: "read", disabled: off, onClick: () => void bulkRun("read", ids) }
        : { label: "Oznacz jako nieprzeczytaną" + n, icon: "unread", disabled: off, onClick: () => void bulkRun("unread", ids) },
      t.starred
        ? { label: "Usuń gwiazdkę" + n, icon: "star", disabled: off, onClick: () => void bulkRun("unstar", ids) }
        : { label: "Dodaj gwiazdkę" + n, icon: "star", disabled: off, onClick: () => void bulkRun("star", ids) },
      "sep",
    );
    for (const p of PRIOS) items.push({ label: "Flaga: " + p.label.toLowerCase() + n, swatch: p.color, checked: level === p.level, onClick: () => ids.forEach((id) => prio.set(key(id), p.level).catch(() => showToast("Nie udało się zapisać flagi."))) });
    if (many || level > 0) items.push({ label: "Usuń flagę" + n, icon: "flag", onClick: () => ids.forEach((id) => prio.set(key(id), 0).catch(() => showToast("Nie udało się zapisać flagi."))) });
    items.push("sep");
    if (folder === "inbox" || folder === "unread") items.push({ label: "Archiwizuj" + n, icon: "archive", disabled: off, onClick: () => void bulkRun("archive", ids) });
    if (folder === "inbox" || folder === "unread") {
      for (const sp of snoozePresets()) items.push({ label: "Odłóż: " + sp.label + n, icon: "clock", disabled: off, onClick: () => void snoozeThreads(ids, sp.ms) });
    }
    if (folder === "archive") items.push({ label: "Przenieś do skrzynki odbiorczej" + n, icon: "inbox", disabled: off, onClick: () => void bulkRun("unarchive", ids) });
    items.push(folder === "spam"
      ? { label: "To nie jest spam" + n, icon: "notspam", disabled: off, onClick: () => void bulkRun("notspam", ids) }
      : { label: "Oznacz jako spam" + n, icon: "spam", disabled: off, onClick: () => void bulkRun("spam", ids) });
    items.push(folder === "trash"
      ? { label: "Przywróć z kosza" + n, icon: "restore", disabled: off, onClick: () => void bulkRun("untrash", ids) }
      : { label: "Usuń" + n, icon: "trash", danger: true, disabled: off, onClick: () => void bulkRun("trash", ids) });
    return items;
  };

  const imapRowMenu = (r: ImapRow): MenuEntry[] => {
    const acc = imapAccount;
    if (!acc) return [];
    const many = imapSel.has(r.uid) && imapSel.size > 1;
    const uids = many ? [...imapSel] : [r.uid];
    const n = many ? ` (${uids.length})` : "";
    const key = (uid: number) => imapPrioKey(acc, imap.folder, uid);
    const level = many ? 0 : prio.get(key(r.uid));
    const items: MenuEntry[] = [];
    if (!many) {
      const go = (cmd: "reply" | "replyAll" | "forward") => () => { setImapPending({ uid: r.uid, cmd }); setImapUid(r.uid); };
      items.push(
        { label: "Odpowiedz", icon: "reply", onClick: go("reply") },
        { label: "Odpowiedz wszystkim", icon: "replyAll", onClick: go("replyAll") },
        { label: "Przekaż dalej", icon: "forward", onClick: go("forward") },
        "sep",
      );
    }
    items.push(
      r.unread
        ? { label: "Oznacz jako przeczytaną" + n, icon: "read", onClick: () => void imapBulk("read", uids) }
        : { label: "Oznacz jako nieprzeczytaną" + n, icon: "unread", onClick: () => void imapBulk("unread", uids) },
      r.starred
        ? { label: "Usuń gwiazdkę" + n, icon: "star", onClick: () => void imapBulk("unstar", uids) }
        : { label: "Dodaj gwiazdkę" + n, icon: "star", onClick: () => void imapBulk("star", uids) },
      "sep",
    );
    for (const p of PRIOS) items.push({ label: "Flaga: " + p.label.toLowerCase() + n, swatch: p.color, checked: level === p.level, onClick: () => uids.forEach((u) => prio.set(key(u), p.level).catch(() => showToast("Nie udało się zapisać flagi."))) });
    if (many || level > 0) items.push({ label: "Usuń flagę" + n, icon: "flag", onClick: () => uids.forEach((u) => prio.set(key(u), 0).catch(() => showToast("Nie udało się zapisać flagi."))) });
    items.push("sep");
    if (imapRole === "inbox") items.push({ label: "Archiwizuj" + n, icon: "archive", onClick: () => void imapBulk("archive", uids) });
    if (!["inbox", "trash", "spam"].includes(imapRole)) items.push({ label: "Przenieś do skrzynki odbiorczej" + n, icon: "inbox", onClick: () => void imapBulk("inbox", uids) });
    items.push(imapRole === "spam"
      ? { label: "To nie jest spam" + n, icon: "notspam", onClick: () => void imapBulk("inbox", uids) }
      : { label: "Oznacz jako spam" + n, icon: "spam", onClick: () => void imapBulk("spam", uids) });
    items.push(imapRole === "trash"
      ? { label: "Przywróć z kosza" + n, icon: "restore", onClick: () => void imapBulk("inbox", uids) }
      : { label: "Usuń" + n, icon: "trash", danger: true, onClick: () => void imapBulk("trash", uids) });
    return items;
  };

  // Polskie menu dla pól tekstowych, zaznaczonego tekstu i linków; domyślnego (angielskiego) menu przeglądarki nie pokazujemy.
  useEffect(() => {
    const h = (e: MouseEvent) => {
      if (e.defaultPrevented) return;
      e.preventDefault();
      const items = textMenuFor(e.target, (t) => showToast(t));
      if (items.length > 0) tm.show(e.clientX, e.clientY, items);
    };
    window.addEventListener("contextmenu", h);
    return () => window.removeEventListener("contextmenu", h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showToast]);

  const imapRowClick = (e: React.MouseEvent, r: ImapRow) => {
    const toggle = (uid: number) => setImapSel((prev) => { const n = new Set(prev); if (n.has(uid)) n.delete(uid); else n.add(uid); return n; });
    if (e.ctrlKey || e.metaKey) { toggle(r.uid); imapAnchor.current = r.uid; return; }
    if (e.shiftKey && imapAnchor.current !== null) {
      const a = imap.rows.findIndex((x) => x.uid === imapAnchor.current);
      const b = imap.rows.findIndex((x) => x.uid === r.uid);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        setImapSel(new Set(imap.rows.slice(lo, hi + 1).map((x) => x.uid)));
        return;
      }
    }
    setImapSel(new Set());
    imapAnchor.current = r.uid;
    setImapUid(r.uid);
  };
  const imapAccountRef = useRef<string | null>(null);
  imapAccountRef.current = imapAccount;
  const MOVE_TEXT: Record<string, string> = {
    trash: "Przeniesiono do kosza.", spam: "Przeniesiono do Spamu.", archive: "Zarchiwizowano.", inbox: "Przeniesiono do Odebranych.",
  };
  // Komunikat po przeniesieniu wiadomości; „Cofnij” jest dostępne, gdy serwer podał nowy numer wiadomości.
  const imapMoved = (account: string, from: string, moved: { uid: number; out: ImapActOut }[], text: string) => {
    const undoable = moved.filter((m) => m.out.folder && m.out.uid);
    showToast(text, undoable.length === 0 ? undefined : () => {
      void (async () => {
        for (const m of undoable) {
          try { await api.imapMove(account, m.out.folder as string, m.out.uid as number, from); } catch { /* pomijamy */ }
        }
        setToast(null);
        if (imapAccountRef.current === account) void imap.refresh();
      })();
    });
  };
  const editImapDraft = (account: string, m: ImapMsg) => {
    if (compose) { showToast("Najpierw zapisz lub zamknij redagowaną wiadomość."); return; }
    const html = m.html ? sanitizeHtml(m.html) : escapeHtml(m.text).replace(/\r?\n/g, "<br>");
    startCompose({
      mode: "draft", to: m.to, cc: m.cc, bcc: "", subject: m.subject, html, threadId: null, inReplyTo: null,
      references: m.references || null, draftId: String(m.uid),
      note: m.attachments.length > 0 ? "Załączniki z tej wersji roboczej trzeba dodać ponownie." : undefined,
    }, "imap:" + account);
  };
  const imapBulk = async (action: string, only?: number[]) => {
    if (!imapAccount) return;
    const account = imapAccount;
    const folderName = imap.folder;
    const uids = (only ?? [...imapSel]).slice(0, 50);
    if (uids.length === 0) return;
    const done: { uid: number; out: ImapActOut }[] = [];
    let failed = 0;
    let lastError = "";
    for (const uid of uids) {
      try { done.push({ uid, out: await api.imapAct(account, folderName, uid, action) }); }
      catch (e) { failed += 1; lastError = errText(e); }
    }
    const isMove = ["trash", "spam", "archive", "inbox"].includes(action);
    if (isMove) {
      for (const d of done) imap.remove(d.uid);
      if (imapUid !== null && done.some((d) => d.uid === imapUid)) setImapUid(null);
    } else {
      for (const d of done) {
        if (action === "star" || action === "unstar") imap.patch(d.uid, { starred: action === "star" });
        else imap.patch(d.uid, { unread: action === "unread" });
      }
    }
    setImapSel(new Set());
    const text = `Gotowe (${done.length}).` + (failed ? ` Błędy: ${failed} – ${lastError}` : "");
    if (isMove && done.length > 0) imapMoved(account, folderName, done, text);
    else showToast(text);
  };

  const composeNode = compose ? (
    <Compose key={compose.key} init={compose.init} from={compose.from} onSent={onSent} onQueued={onQueued} onScheduled={onScheduled} onClose={onComposeClosed}
      senders={email ? [{ id: "gmail", label: email }, ...imapAccs.map((a) => ({ id: "imap:" + a.email, label: a.email }))] : undefined} />
  ) : null;

  return (
    <div className="app">
      {gm.menu}
      {tm.menu}
      <Titlebar center={email ? (
        <TitleSearch query={query} onQuery={setQuery} filters={filters} onFilters={setFilters} error={filtErr}
          open={optsOpen} onOpen={setOptsOpen} onSubmit={doSearch}
          active={imapAccount ? Boolean(imap.term) : searchTerm !== null}
          onClear={() => {
            if (imapAccount) { imap.clearSearch(); setQuery(""); setFilters(EMPTY_FILTERS); setFiltErr(null); }
            else { clearSearch(); void loadList(folder, null, false); }
          }}
          placeholder={imapAccount ? "Szukaj w tym folderze…" : "Szukaj w poczcie…"} />
      ) : undefined}>
        {email && (
          <AccountSwitcher gmail={accounts.length ? accounts : [{ email, active: true }]} imap={imapAccs} view={view}
            label={imapAccount ?? email} busy={signingIn}
            onGmail={(a) => { changeView("gmail"); if (a !== email) void switchAccount(a); }}
            onImap={(a) => changeView("imap:" + a)} />
        )}
      </Titlebar>
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
        </aside>

        <div className="handle" onPointerDown={left.onPointerDown} role="separator" aria-orientation="vertical" />

        {imapAccount ? (
          <>
            <ImapListPane imap={imap} role={imapRole} openUid={imapUid} selected={imapSel} onRowClick={imapRowClick}
              onSelectAll={() => setImapSel(new Set(imap.rows.map((r) => r.uid)))} onClear={() => setImapSel(new Set())}
              onBulk={(a) => void imapBulk(a)} rowMenu={imapRowMenu} />
            <div className="handle" onPointerDown={mid.onPointerDown} role="separator" aria-orientation="vertical" />
            {compose ? <main className="reader" aria-label="Nowa wiadomość">{composeNode}</main> : <ImapReader account={imapAccount} folder={imap.folder} role={imapRole} uid={imapUid} keys={imapSel.size === 0} pending={imapPending} onPendingDone={() => setImapPending(null)}
              onCompose={(k, m) => composeFromImap(imapAccount, k, m)}
              onEditDraft={(m) => editImapDraft(imapAccount, m)}
              onMoved={(i) => imapMoved(imapAccount, i.folder, [{ uid: i.uid, out: i.out }], MOVE_TEXT[i.action] ?? "Gotowe.")}
              onChanged={(uid, p) => { if (p === "removed") { imap.remove(uid); setImapUid(null); } else imap.patch(uid, p); }} />}
          </>
        ) : (
        <>
        <section className="list" aria-label="Lista konwersacji">
          <header className="list-head">
            <h2>{searchTerm ? `Wyniki: „${searchTerm}”` : folderLabel}</h2>
            {email && <FlagsFirstToggle />}
            {email && (
              <ReceiveButton busy={loadingList} onClick={() => void loadList(folder, null, false)} />
            )}
          </header>
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
              {selected.size > 0 && (
                <div className="bulkbar" role="toolbar" aria-label="Akcje na zaznaczonych">
                  <strong>Zaznaczono: {selected.size}</strong>
                  <button className="small" title="Zaznacz wszystkie na liście (Ctrl+A)" aria-label="Zaznacz wszystkie na liście (Ctrl+A)" onClick={() => setSelected(new Set(threads.map((t) => t.id)))}><ActIcon n="selectAll"/></button>
                  {folder !== "archive" && folder !== "trash" && folder !== "spam" && (
                    <button className="small" title="Archiwizuj zaznaczone (e)" aria-label="Archiwizuj zaznaczone (e)" disabled={needsReauth} onClick={() => void bulkRun("archive")}><ActIcon n="archive"/></button>
                  )}
                  {folder === "trash"
                    ? <button className="small" title="Przywróć zaznaczone z kosza" aria-label="Przywróć zaznaczone z kosza" disabled={needsReauth} onClick={() => void bulkRun("untrash")}><ActIcon n="restore"/></button>
                    : <button className="small" title="Usuń zaznaczone – przenieś do kosza (Delete)" aria-label="Usuń zaznaczone – przenieś do kosza (Delete)" disabled={needsReauth} onClick={() => void bulkRun("trash")}><ActIcon n="trash"/></button>}
                  {folder === "spam"
                    ? <button className="small" title="To nie spam – przenieś zaznaczone do skrzynki" aria-label="To nie spam – przenieś zaznaczone do skrzynki" disabled={needsReauth} onClick={() => void bulkRun("notspam")}><ActIcon n="notspam"/></button>
                    : <button className="small" title="Oznacz zaznaczone jako spam (!)" aria-label="Oznacz zaznaczone jako spam (!)" disabled={needsReauth} onClick={() => void bulkRun("spam")}><ActIcon n="spam"/></button>}
                  <button className="small" title="Oznacz zaznaczone jako przeczytane (Shift+I)" aria-label="Oznacz zaznaczone jako przeczytane (Shift+I)" disabled={needsReauth} onClick={() => void bulkRun("read")}><ActIcon n="read"/></button>
                  <button className="small" title="Oznacz zaznaczone jako nieprzeczytane (Shift+U)" aria-label="Oznacz zaznaczone jako nieprzeczytane (Shift+U)" disabled={needsReauth} onClick={() => void bulkRun("unread")}><ActIcon n="unread"/></button>
                  <button className="small" title="Odznacz wszystko (Esc)" aria-label="Odznacz wszystko (Esc)" onClick={() => setSelected(new Set())}><ActIcon n="clear"/></button>
                </div>
              )}
              <ul>
                {threads.map((t) => {
                  return (
                  <li key={t.id} className={selected.has(t.id) ? "picked" : ""}>
                    <button data-thread={t.id}
                      className={"row" + (t.id === openId ? " active" : "") + (t.unread ? " unread" : "")}
                      aria-pressed={selected.has(t.id)}
                      onClick={(e) => onRowClick(e, t)} {...gm.bind(() => gmailRowMenu(t))}>
                      <span className="who">{t.unread && <i className="udot" role="img" aria-label="Nieprzeczytana" />}{t.participants}{t.count > 1 ? ` (${t.count})` : ""}</span>
                      <span className="when">
                        {email && <PrioFlag level={prioLevels[gmailPrioKey(email, t.id)] ?? 0} />} {t.starred && <Icon d={STAR} />} {fmtDate(t.lastDateMs)}
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
                  <button title="Odpowiedz nadawcy (r)" aria-label="Odpowiedz nadawcy (r)" className="primary" disabled={needsReauth} onClick={() => reply(false)}><ActIcon n="reply"/></button>
                  <button title="Odpowiedz wszystkim (a)" aria-label="Odpowiedz wszystkim (a)" disabled={needsReauth} onClick={() => reply(true)}><ActIcon n="replyAll"/></button>
                  <button title="Przekaż dalej (f)" aria-label="Przekaż dalej (f)" disabled={needsReauth} onClick={forward}><ActIcon n="forward"/></button>
                  <span className="sep" />
                  {detail.inInbox && <button title="Odłóż na później" aria-label="Odłóż na później" disabled={needsReauth}
                    onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); gm.show(r.left, r.bottom + 4, snoozePresets().map((sp) => ({ label: "Odłóż: " + sp.label, icon: "clock" as const, onClick: () => void snoozeThreads([detail.id], sp.ms) }))); }}><ActIcon n="clock"/></button>}
                  {detail.inInbox && <button title="Archiwizuj (e)" aria-label="Archiwizuj (e)" disabled={needsReauth} onClick={() => void runAction("archive")}><ActIcon n="archive"/></button>}
                  {!detail.inInbox && !detail.inTrash && !detail.inSpam && (
                    <button title="Przenieś do skrzynki odbiorczej" aria-label="Przenieś do skrzynki odbiorczej" disabled={needsReauth} onClick={() => void runAction("unarchive")}><ActIcon n="inbox"/></button>
                  )}
                  {detail.inSpam
                    ? <button title="To nie jest spam – przenieś do skrzynki" aria-label="To nie jest spam – przenieś do skrzynki" disabled={needsReauth} onClick={() => void runAction("notspam")}><ActIcon n="notspam"/></button>
                    : <button title="Oznacz jako spam (!)" aria-label="Oznacz jako spam (!)" disabled={needsReauth} onClick={() => void runAction("spam")}><ActIcon n="spam"/></button>}
                  {detail.inTrash
                    ? <button title="Przywróć z kosza" aria-label="Przywróć z kosza" disabled={needsReauth} onClick={() => void runAction("untrash")}><ActIcon n="restore"/></button>
                    : <button title="Usuń – przenieś do kosza (Delete)" aria-label="Usuń – przenieś do kosza (Delete)" disabled={needsReauth} onClick={() => void runAction("trash")}><ActIcon n="trash"/></button>}
                  <button title={detail.unread ? "Oznacz jako przeczytaną (Shift+I)" : "Oznacz jako nieprzeczytaną (Shift+U)"} aria-label={detail.unread ? "Oznacz jako przeczytaną (Shift+I)" : "Oznacz jako nieprzeczytaną (Shift+U)"} disabled={needsReauth} onClick={() => void runAction(detail.unread ? "read" : "unread")}>
                    <ActIcon n={detail.unread ? "read" : "unread"} /></button>
                  {stats.labels.length > 0 && (
                    <details className="label-menu">
                      <summary title="Etykiety" aria-label="Etykiety"><ActIcon n="label" /></summary>
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
                  <button title={detail.starred ? "Usuń gwiazdkę (s)" : "Dodaj gwiazdkę (s)"} aria-label={detail.starred ? "Usuń gwiazdkę (s)" : "Dodaj gwiazdkę (s)"} disabled={needsReauth} aria-pressed={detail.starred} onClick={() => void runAction(detail.starred ? "unstar" : "star")}>
                    <ActIcon n="star" filled={detail.starred} /></button>
                  {email && <PrioMenu itemKey={gmailPrioKey(email, detail.id)} onError={showToast} />}
                  {(() => {
                    const t = replyTarget(detail.messages);
                    return t && !t.mine ? (
                      <button title="Opcje nadawcy" aria-label="Opcje nadawcy" onClick={() => setSenderDlg({ email: t.fromEmail })}><ActIcon n="user"/></button>
                    ) : null;
                  })()}
                  <span className="sep" />
                  <button title="Rozwiń wszystkie wiadomości w wątku" aria-label="Rozwiń wszystkie wiadomości w wątku" onClick={() => setCollapsed(new Set())}><ActIcon n="expand"/></button>
                  <button title="Zwiń wszystkie wiadomości w wątku" aria-label="Zwiń wszystkie wiadomości w wątku" onClick={() => setCollapsed(new Set(detail.messages.map((m) => m.id)))}><ActIcon n="collapse"/></button>
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
        </>
        )}
      </div>

      <StatusBar info={info} u={updater} outboxCount={email ? outbox.length + scheduled.length + snoozed.length : 0} onOutbox={() => setOutboxOpen(true)}
        onSettings={() => setSettingsOpen(true)} />

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
                {shortcutHelp().map((k) => (
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
        <OutboxDialog items={outbox} scheduled={scheduled} snoozed={snoozed}
          onCancelScheduled={async (id) => { try { await api.scheduleDelete(imapAccount, id); } catch (e) { showToast(errText(e)); } void refreshLater(); }}
          onWake={(id) => void wakeSnoozed(id)} onClose={() => setOutboxOpen(false)} onSendNow={() => void flushQueue(true)}
          onRetry={async (id) => { try { await outboxOps(imapAccount).retry(id); } catch (e) { showToast(errText(e)); } await flushQueue(true); }}
          onDelete={async (id) => { try { await outboxOps(imapAccount).del(id); } catch (e) { showToast(errText(e)); } await flushQueue(false); }} />
      )}
      {settingsOpen && (
        <SettingsDialog notify={notifyOn} content={notifyContent} background={background} onBackground={(on) => { setBackground(on); save(LS.bg, on ? "1" : "0"); }} gmail={accounts.length ? accounts : email ? [{ email, active: true }] : []} imap={imapAccs} busy={signingIn}
          activeOnly={activeOnly} onActiveOnly={(on) => { setActiveOnly(on); save(LS.active, on ? "1" : "0"); }}
          onSignOut={() => { setSettingsOpen(false); void signOut(); }} onSignOutImap={(a) => { setSettingsOpen(false); void signOutImap(a); }}
          autoUpdate={autoUpdate} onAutoUpdate={toggleAuto} hasPrevious={!!updater.state?.hasPrevious}
          onRollback={() => void updater.rollback()} onCheckUpdate={() => void updater.check(true)}
          theme={theme} onTheme={setTheme}
          onKeysChanged={() => save(LS.keys, JSON.stringify(customKeys()))}
          sound={sound} onSound={(s) => { setSound(s); saveSound(s); }}
          onNotify={(on) => { setNotifyOn(on); save(LS.notify, on ? "1" : "0"); }}
          onContent={(on) => { setNotifyContent(on); save(LS.content, on ? "1" : "0"); }}
          onOpen={(w) => { setSettingsOpen(false); if (w === "signature") setSigOpen(true); else if (w === "senders") setSenderDlg({ email: null }); else if (w === "spam") setSpamOpen(true); else if (w === "import") setImportOpen(true); else if (w === "addGmail") void signIn(); else if (w === "addImap") setImapLoginOpen(true); else setCacheOpen(true); }}
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
