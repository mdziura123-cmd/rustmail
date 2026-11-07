import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { AuthPanel } from "./AuthPanel";
import { FOLDERS, type FolderId } from "./folders";
import { UpdateBanner } from "./UpdateBanner";
import { useUpdater } from "./useUpdater";
import { fmtDate, fmtFull, fmtSize } from "./util";
import type { AppInfo, AuthStatus, ThreadDetail, ThreadSummary } from "./types";
import "./styles.css";

type Theme = "auto" | "light" | "dark";
const LS = { theme: "rm.theme", left: "rm.left", mid: "rm.mid", auto: "rm.autoupdate" };
const STAR = "M12 3l2.9 6 6.6.9-4.8 4.6 1.2 6.5L12 17.8 6.1 21l1.2-6.5L2.5 9.9 9.1 9z";

function load(key: string, fallback: string): string {
  try { return localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}
function save(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* pamięć niedostępna – ignorujemy */ }
}
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
      setAuth({ configured: false, email: null });
    }
  }, []);
  useEffect(() => { void refreshAuth(); }, [refreshAuth]);

  const loadList = useCallback(async (f: FolderId, token: string | null, append: boolean) => {
    const my = ++listReq.current;
    setLoadingList(true);
    setListError(null);
    try {
      const page = await api.listThreads(f, token);
      if (my !== listReq.current) return;
      setThreads((prev) => (append ? [...prev, ...page.threads] : page.threads));
      setNextToken(page.nextPageToken);
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

  const openThread = async (t: ThreadSummary) => {
    const my = ++threadReq.current;
    setOpenId(t.id);
    setDetail(null);
    setDetailError(null);
    setDetailLoading(true);
    setCollapsed(new Set());
    try {
      const d = await api.getThread(t.id);
      if (my === threadReq.current) setDetail(d);
    } catch (e) {
      if (my === threadReq.current) setDetailError(errText(e));
    } finally {
      if (my === threadReq.current) setDetailLoading(false);
    }
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

  const folderLabel = FOLDERS.find((f) => f.id === folder)?.label ?? "";

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
          <button className="primary" disabled title="Redagowanie pojawi się w etapie 3">Nowa wiadomość</button>
          <nav>
            {FOLDERS.map((f) => (
              <button key={f.id} className={"folder" + (f.id === folder ? " active" : "")}
                onClick={() => setFolder(f.id)} aria-current={f.id === folder}>
                <Icon d={f.icon} />
                <span>{f.label}</span>
              </button>
            ))}
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
            <h2>{folderLabel}</h2>
            {email && (
              <button className="small" disabled={loadingList} onClick={() => void loadList(folder, null, false)}>
                {loadingList ? "Wczytywanie…" : "Odśwież"}
              </button>
            )}
          </header>
          {!email ? (
            <div className="empty">
              <h3>Nie połączono konta</h3>
              <p>Zaloguj się po prawej stronie, aby zobaczyć pocztę.</p>
            </div>
          ) : (
            <>
              {listError && <p className="error pad" role="alert">{listError}</p>}
              {!loadingList && !listError && threads.length === 0 && (
                <div className="empty">
                  <h3>Ten folder jest pusty</h3>
                  {folder === "suspicious" && <p>Folder „Podejrzane” zostanie wypełniony po wdrożeniu filtrów bezpieczeństwa (etap 5).</p>}
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
                      <span className="subj">{t.subject}</span>
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
          {!auth ? (
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
                <div className="tools">
                  <button onClick={() => setCollapsed(new Set())}>Rozwiń wszystkie</button>
                  <button onClick={() => setCollapsed(new Set(detail.messages.map((m) => m.id)))}>Zwiń wszystkie</button>
                </div>
              </header>
              <div className="thread">
                {detail.messages.map((m) => {
                  const folded = collapsed.has(m.id);
                  return (
                    <article key={m.id} className={"bubble " + (m.mine ? "sent" : "received")}>
                      <button className="bubble-head" onClick={() => toggleMsg(m.id)} aria-expanded={!folded}>
                        <strong title={m.fromEmail}>{m.from}</strong>
                        <time>{fmtFull(m.dateMs)}</time>
                      </button>
                      {folded ? (
                        <p className="fold">{m.body.slice(0, 120)}</p>
                      ) : (
                        <>
                          {m.htmlOnly && (
                            <p className="note">Wiadomość HTML pokazana jako uproszczony tekst. Bezpieczne renderowanie pełnego HTML pojawi się w etapie 5.</p>
                          )}
                          <p className="msg">{m.body || "(brak treści tekstowej)"}</p>
                          {m.attachments.map((a) => (
                            <div key={a.name + a.size} className="attach">{a.name} · {fmtSize(a.size)}</div>
                          ))}
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
    </div>
  );
}
