import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Własny pasek tytułu w kolorach motywu (systemowy pasek jest wyłączony w tauri.conf.json). */
export default function Titlebar({ children, center }: { children?: ReactNode; center?: ReactNode }) {
  const [max, setMax] = useState(false);

  useEffect(() => {
    let off: (() => void) | undefined;
    let dead = false;
    try {
      const w = getCurrentWindow();
      const sync = () => void w.isMaximized().then((m) => !dead && setMax(m)).catch(() => {});
      sync();
      void w.onResized(sync).then((u) => (dead ? u() : (off = u))).catch(() => {});
    } catch {
      /* poza Tauri (np. podgląd w przeglądarce) */
    }
    return () => {
      dead = true;
      off?.();
    };
  }, []);

  const run = (f: (w: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => {
    try {
      void f(getCurrentWindow()).catch(() => {});
    } catch {
      /* ignoruj */
    }
  };

  return (
    <div className="titlebar" data-tauri-drag-region>
      <span className="tb-title" data-tauri-drag-region>RustMail</span>
      <div className="tb-center">{center}</div>
      <div className="tb-right">
        {children}
      <div className="tb-btns">
        <button className="tb-btn" aria-label="Minimalizuj" title="Minimalizuj" onClick={run((w) => w.minimize())}>
          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2 6.5h8" stroke="currentColor" strokeWidth="1.4" fill="none" /></svg>
        </button>
        <button className="tb-btn" aria-label={max ? "Przywróć" : "Maksymalizuj"} title={max ? "Przywróć" : "Maksymalizuj"} onClick={run((w) => w.toggleMaximize())}>
          {max ? (
            <svg width="12" height="12" viewBox="0 0 12 12"><path d="M4 2.5h5.5V8M2.5 4H8v5.5H2.5z" stroke="currentColor" strokeWidth="1.2" fill="none" /></svg>
          ) : (
            <svg width="12" height="12" viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" stroke="currentColor" strokeWidth="1.3" fill="none" /></svg>
          )}
        </button>
        <button className="tb-btn tb-close" aria-label="Zamknij" title="Zamknij" onClick={run((w) => w.close())}>
          <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" strokeWidth="1.4" fill="none" /></svg>
        </button>
      </div>
      </div>
    </div>
  );
}
