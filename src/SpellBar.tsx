import { useEffect, useRef, useState } from "react";
import { ActIcon } from "./ActIcon";
import type { EditorApi } from "./RichEditor";
import { addCustomWord, checkWords, ensureSpell, extractWords, suggestWord, type SpellLang } from "./spell";

const LANG_KEY = "rm.spell";
type Mode = SpellLang | "off";

function loadMode(): Mode {
  try {
    const v = localStorage.getItem(LANG_KEY);
    return v === "en" || v === "off" ? v : "pl";
  } catch { return "pl"; }
}

interface Props {
  getHtml: () => string;
  /** Zmienia się po każdej zmianie treści – uruchamia (z opóźnieniem) ponowne sprawdzenie. */
  tick: number;
  api: { current: EditorApi | null };
}

/** Pasek pisowni pod edytorem: sprawdza tekst w trakcie pisania i podpowiada poprawki. */
export function SpellBar({ getHtml, tick, api }: Props) {
  const [mode, setMode] = useState<Mode>(loadMode);
  const [status, setStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const [bad, setBad] = useState<string[]>([]);
  const [sugg, setSugg] = useState<Record<string, string[]>>({});
  const ignored = useRef<Set<string>>(new Set());
  const req = useRef(0);
  const [menu, setMenu] = useState<{ w: string; x: number; y: number } | null>(null);
  const badRef = useRef<string[]>([]);
  badRef.current = bad;

  const changeMode = (m: Mode) => {
    setMode(m);
    try { localStorage.setItem(LANG_KEY, m); } catch { /* pamięć niedostępna */ }
  };

  // Wczytanie słownika po wyborze języka.
  useEffect(() => {
    setBad([]);
    setSugg({});
    api.current?.highlight([]);
    if (mode === "off") { setStatus("idle"); return; }
    let live = true;
    setStatus("loading");
    setError(null);
    ensureSpell(mode).then(() => { if (live) setStatus("ready"); })
      .catch((e) => { if (live) { setError(e instanceof Error ? e.message : String(e)); setStatus("error"); } });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  const showBad = (list: string[]) => {
    setBad(list);
    api.current?.highlight(list);
    for (const w of list.slice(0, 4)) {
      setSugg((s) => {
        if (s[w]) return s;
        void suggestWord(w).then((r) => setSugg((x) => ({ ...x, [w]: r }))).catch(() => {});
        return s;
      });
    }
  };

  // Sprawdzanie z opóźnieniem po przerwie w pisaniu.
  useEffect(() => {
    if (mode === "off" || status !== "ready") return;
    const my = ++req.current;
    const t = setTimeout(() => {
      const words = extractWords(getHtml()).filter((w) => !ignored.current.has(w));
      checkWords(words).then((b) => { if (my === req.current) showBad(b); }).catch(() => {});
    }, 700);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tick, mode, status]);

  const drop = (w: string) => {
    const rest = bad.filter((x) => x !== w);
    setBad(rest);
    api.current?.highlight(rest);
  };
  const fix = (w: string, to: string) => { api.current?.replaceWord(w, to); drop(w); };
  const ignore = (w: string) => { ignored.current.add(w); drop(w); };
  const learn = (w: string) => { void addCustomWord(w).then(() => drop(w)).catch(() => {}); };
  const ask = (w: string) => { void suggestWord(w).then((r) => setSugg((x) => ({ ...x, [w]: r }))).catch(() => {}); };
  const keep = (e: React.MouseEvent) => e.preventDefault();

  // Menu pod prawym przyciskiem myszy na błędnym słowie w edytorze.
  const askRef = useRef(ask);
  askRef.current = ask;
  useEffect(() => {
    const handler = (w: string, x: number, y: number) => {
      if (!badRef.current.includes(w)) return false;
      setMenu({ w, x, y });
      askRef.current(w);
      return true;
    };
    if (api.current) api.current.contextHandler = handler;
    return () => { if (api.current) api.current.contextHandler = null; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const down = (e: MouseEvent) => { if (!(e.target as HTMLElement | null)?.closest(".spell-menu")) close(); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, [menu]);
  const act = (f: () => void) => { setMenu(null); f(); };

  return (
    <div className="spellbar" role="region" aria-label="Sprawdzanie pisowni">
      <label className="spell-lang">
        Pisownia:{" "}
        <select value={mode} onChange={(e) => changeMode(e.target.value as Mode)}>
          <option value="pl">polska</option>
          <option value="en">angielska</option>
          <option value="off">wyłączona</option>
        </select>
      </label>
      {status === "loading" && <span className="hint">Wczytywanie słownika…</span>}
      {status === "error" && <span className="hint">Sprawdzanie pisowni niedostępne: {error}</span>}
      {status === "ready" && bad.length === 0 && <span className="hint">Nie znaleziono błędów.</span>}
      {status === "ready" && bad.slice(0, 6).map((w) => (
        <span className="spell-chip" key={w}>
          <b>{w}</b>
          {sugg[w] === undefined ? (
            <button type="button" className="small" onMouseDown={keep} onClick={() => ask(w)}>Podpowiedz</button>
          ) : sugg[w].length === 0 ? (
            <span className="hint">brak podpowiedzi</span>
          ) : (
            sugg[w].slice(0, 3).map((s) => (
              <button type="button" className="small solid" key={s} onMouseDown={keep} onClick={() => fix(w, s)}>{s}</button>
            ))
          )}
          <button type="button" className="small" title="Dodaj do słownika" aria-label={`Dodaj „${w}” do słownika`} onMouseDown={keep} onClick={() => learn(w)}><ActIcon n="dict" /></button>
          <button type="button" className="small" title="Ignoruj" aria-label={`Ignoruj „${w}”`} onMouseDown={keep} onClick={() => ignore(w)}><ActIcon n="clear" /></button>
        </span>
      ))}
      {status === "ready" && bad.length > 6 && <span className="hint">…i {bad.length - 6} więcej</span>}
      {menu && (
        <div className="spell-menu" role="menu" aria-label={`Poprawki dla słowa ${menu.w}`}
          style={{ left: Math.max(4, Math.min(menu.x, window.innerWidth - 230)), top: Math.max(4, Math.min(menu.y, window.innerHeight - 230)) }}>
          {sugg[menu.w] === undefined && <div className="spell-menu-note">Szukam podpowiedzi…</div>}
          {sugg[menu.w] !== undefined && sugg[menu.w].length === 0 && <div className="spell-menu-note">Brak podpowiedzi</div>}
          {(sugg[menu.w] ?? []).slice(0, 6).map((s) => (
            <button type="button" role="menuitem" key={s} className="spell-menu-item strong" onMouseDown={keep} onClick={() => act(() => fix(menu.w, s))}>{s}</button>
          ))}
          <div className="spell-menu-sep" />
          <button type="button" role="menuitem" className="spell-menu-item" onMouseDown={keep} onClick={() => act(() => learn(menu.w))}>Dodaj do słownika</button>
          <button type="button" role="menuitem" className="spell-menu-item" onMouseDown={keep} onClick={() => act(() => ignore(menu.w))}>Ignoruj</button>
        </div>
      )}
    </div>
  );
}
