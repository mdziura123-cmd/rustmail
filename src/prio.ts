import { useSyncExternalStore } from "react";
import { api } from "./api";

// Wspólny, lokalny magazyn flag ważności dla list Gmaila i kont IMAP. Dane trwałe są w zaszyfrowanej
// bazie lokalnej (backend); tu trzymamy tylko to, co już wczytano.
let levels: Record<string, number> = {};
const known = new Set<string>();
const subs = new Set<() => void>();
const emit = () => { levels = { ...levels }; subs.forEach((f) => f()); };

export const prio = {
  get: (key: string): number => levels[key] ?? 0,

  /** Wczytuje flagi dla podanych kluczy (tylko te, których jeszcze nie znamy). */
  async ensure(keys: string[]): Promise<void> {
    const need = Array.from(new Set(keys.filter((k) => !known.has(k))));
    if (need.length === 0) return;
    try {
      for (let i = 0; i < need.length; i += 200) {
        const chunk = need.slice(i, i + 200);
        const got = await api.prioGet(chunk);
        for (const k of chunk) { known.add(k); if (got[k]) levels[k] = got[k]; else delete levels[k]; }
      }
      emit();
    } catch { /* baza lokalna niedostępna – flagi po prostu się nie pokażą */ }
  },

  /** Ustawia flagę (0 usuwa). Błąd zapisu jest zgłaszany wywołującemu. */
  async set(key: string, level: number): Promise<void> {
    await api.prioSet(key, level);
    known.add(key);
    if (level > 0) levels[key] = level; else delete levels[key];
    emit();
  },
};

/** Odświeża komponent po każdej zmianie flag; zwraca bieżącą mapę kluczy na poziomy. */
export function usePrioLevels(): Record<string, number> {
  return useSyncExternalStore(
    (cb) => { subs.add(cb); return () => { subs.delete(cb); }; },
    () => levels,
  );
}
