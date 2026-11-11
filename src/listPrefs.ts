import { useSyncExternalStore } from "react";

/** Ustawienie „flagi na górze listy” (także poza wyszukiwaniem). Zapamiętywane w tej przeglądarce. */
const KEY = "rm.flagsfirst";
const subs = new Set<() => void>();
let on = (() => { try { return localStorage.getItem(KEY) === "1"; } catch { return false; } })();

export const flagsFirst = () => on;
export function setFlagsFirst(v: boolean) {
  on = v;
  try { localStorage.setItem(KEY, v ? "1" : "0"); } catch { /* pamięć niedostępna */ }
  subs.forEach((f) => f());
}
const sub = (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; };
export const useFlagsFirst = () => useSyncExternalStore(sub, flagsFirst);
