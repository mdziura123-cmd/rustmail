import { useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { CalendarDialog, type CalPrefill } from "./Calendar";
import { isThemeId, resolveTheme } from "./themes";
import { bindAppLock, useAppLock } from "./appLock";
import { LockScreen } from "./LockScreen";

function parse(raw: string | null): CalPrefill | null {
  if (!raw) return null;
  try {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== "object") return null;
    const o = v as Record<string, unknown>;
    if (typeof o.title !== "string" || typeof o.date !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(o.date)) return null;
    return {
      title: o.title.slice(0, 120),
      notes: typeof o.notes === "string" ? o.notes.slice(0, 2000) : "",
      date: o.date,
      time: typeof o.time === "string" && /^\d{2}:\d{2}$/.test(o.time) ? o.time : undefined,
    };
  } catch { return null; }
}

/** Zawartość osobnego okna kalendarza (okno o etykiecie „calendar”). */
export default function CalendarWindow() {
  const [prefill, setPrefill] = useState<CalPrefill | null>(null);
  const [gen, setGen] = useState(0);
  const { locked } = useAppLock();
  // Okno kalendarza dzieli blokadę z oknem poczty (nie blokuje się samo przy otwarciu).
  useEffect(() => { bindAppLock(false); }, []);
  // Praca w kalendarzu liczy się jako aktywność: okno poczty nie zablokuje aplikacji z powodu bezczynności.
  useEffect(() => {
    let last = 0;
    const touch = () => {
      const now = Date.now();
      if (now - last < 20_000) return;
      last = now;
      void emit("app-activity").catch(() => undefined);
    };
    const evs = ["pointerdown", "pointermove", "keydown", "wheel", "touchstart"] as const;
    evs.forEach((e) => window.addEventListener(e, touch, { passive: true }));
    return () => evs.forEach((e) => window.removeEventListener(e, touch));
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      let t = "auto";
      try { t = localStorage.getItem("rm.theme") ?? "auto"; } catch { /* domyślny */ }
      const r = resolveTheme(isThemeId(t) ? t : "auto", mq.matches);
      root.setAttribute("data-theme", r.id);
      root.setAttribute("data-mode", r.mode);
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    void api.calTakePrefill().then((p) => { const v = parse(p); if (v) { setPrefill(v); setGen((n) => n + 1); } }).catch(() => undefined);
    void listen<string>("cal-prefill", (e) => { const v = parse(e.payload); if (v) { setPrefill(v); setGen((n) => n + 1); } }).then((f) => { un = f; });
    return () => un?.();
  }, []);

  return (
    <>
      <CalendarDialog key={gen} standalone prefill={prefill} onClose={() => void api.closeCalendarWindow()} />
      {locked && <LockScreen />}
    </>
  );
}
