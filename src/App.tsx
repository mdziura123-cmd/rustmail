import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FOLDERS, type FolderId } from "./folders";
import { PREVIEW_THREADS } from "./previewData";
import type { AppInfo, Thread } from "./types";
import "./styles.css";

type Theme = "auto" | "light" | "dark";
const LS = { theme: "rm.theme", left: "rm.left", mid: "rm.mid" };
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
  const [preview, setPreview] = useState(false);
  const [threads, setThreads] = useState<Thread[]>([]);
  const [openId, setOpenId] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const left = useResizer(LS.left, 240, 180, 360);
  const mid = useResizer(LS.mid, 360, 260, 560);
  const mounted = useRef(false);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "auto") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
    if (mounted.current) save(LS.theme, theme);
    mounted.current = true;
  }, [theme]);

  useEffect(() => {
    invoke<AppInfo>("app_info").then(setInfo).catch(() => setInfo(null));
  }, []);

  const open = threads.find((t) => t.id === openId) ?? null;
  const visible = folder === "inbox" ? threads : [];

  const togglePreview = () => {
    setThreads(preview ? [] : PREVIEW_THREADS);
    setPreview(!preview);
    setOpenId(null);
    setCollapsed(new Set());
  };
  const toggleStar = (id: string) =>
    setThreads((ts) => ts.map((t) => (t.id === id ? { ...t, starred: !t.starred } : t)));
  const toggleMsg = (id: string) =>
    setCollapsed((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const openThread = (t: Thread) => {
    setOpenId(t.id);
    setThreads((ts) => ts.map((x) => (x.id === t.id ? { ...x, unread: false } : x)));
  };

  const folderLabel = FOLDERS.find((f) => f.id === folder)?.label ?? "";

  return (
    <div className="app">
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
          <header><h2>{folderLabel}</h2></header>
          {visible.length === 0 ? (
            <div className="empty">
              <h3>{threads.length === 0 ? "Nie połączono jeszcze konta" : "Ten folder jest pusty"}</h3>
              <p>{threads.length === 0
                ? "Logowanie do Gmaila pojawi się w kolejnym etapie. Do tego czasu możesz obejrzeć sam układ okna."
                : "Brak konwersacji do wyświetlenia."}</p>
              <button disabled title="Logowanie OAuth pojawi się w etapie 2">Dodaj konto Gmail</button>
            </div>
          ) : (
            <ul>
              {visible.map((t) => (
                <li key={t.id}>
                  <div className={"row" + (t.id === openId ? " active" : "") + (t.unread ? " unread" : "")}>
                    <button className="row-main" onClick={() => openThread(t)}>
                      <span className="who">{t.participants}</span>
                      <span className="when">{t.date}</span>
                      <span className="subj">{t.subject}</span>
                      <span className="snip">{t.snippet}</span>
                    </button>
                    <button className={"star" + (t.starred ? " on" : "")} aria-label="Oznacz gwiazdką"
                      aria-pressed={t.starred} onClick={() => toggleStar(t.id)}>
                      <Icon d={STAR} />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>

        <div className="handle" onPointerDown={mid.onPointerDown} role="separator" aria-orientation="vertical" />

        <main className="reader" aria-label="Konwersacja">
          {!open ? (
            <div className="empty"><p>Wybierz konwersację z listy.</p></div>
          ) : (
            <>
              <header className="reader-head">
                <h2>{open.subject}</h2>
                <div className="tools">
                  <button onClick={() => setCollapsed(new Set())}>Rozwiń wszystkie</button>
                  <button onClick={() => setCollapsed(new Set(open.messages.map((m) => m.id)))}>Zwiń wszystkie</button>
                </div>
              </header>
              <div className="thread">
                {open.messages.map((m) => {
                  const isCollapsed = collapsed.has(m.id);
                  return (
                    <article key={m.id} className={"bubble " + (m.mine ? "sent" : "received")}>
                      <button className="bubble-head" onClick={() => toggleMsg(m.id)} aria-expanded={!isCollapsed}>
                        <strong>{m.from}</strong><time>{m.date}</time>
                      </button>
                      {isCollapsed ? <p className="fold">{m.body}</p> : (
                        <>
                          <p>{m.body}</p>
                          {m.attachments?.map((a) => <div key={a} className="attach">{a}</div>)}
                        </>
                      )}
                    </article>
                  );
                })}
              </div>
            </>
          )}
        </main>
      </div>

      <footer className="status">
        <span>{info ? `${info.name} ${info.version} · ${info.platform}${info.appimage ? " · AppImage" : ""}` : "Tryb podglądu w przeglądarce"}</span>
        <span className="pill">Offline · brak konta</span>
        <button className="linklike" onClick={togglePreview}>
          {preview ? "Ukryj dane przykładowe" : "Pokaż układ z danymi przykładowymi"}
        </button>
        {preview && <span className="warn">To dane przykładowe, nie z Gmaila.</span>}
      </footer>
    </div>
  );
}
