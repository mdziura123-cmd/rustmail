// Blokada aplikacji kodem PIN: wspólny stan (zablokowana / odblokowana) i ustawienia czasu bezczynności.
// Sam kod sprawdza backend (skrót PBKDF2 w systemowym magazynie haseł); tu nie ma żadnych sekretów.
import { useSyncExternalStore } from "react";

const KEY_IDLE = "rm.lockidle";
/** Minuty bezczynności przed blokadą; 0 = tylko ręcznie i przy uruchomieniu. */
export const IDLE_CHOICES = [0, 1, 5, 10, 15, 30, 60] as const;

function read(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* pamięć niedostępna */ }
}

export function getIdleMinutes(): number {
  const raw = read(KEY_IDLE);
  if (raw === null) return 10;
  const n = Number(raw);
  return (IDLE_CHOICES as readonly number[]).includes(n) ? n : 10;
}
export function setIdleMinutes(n: number) {
  write(KEY_IDLE, String((IDLE_CHOICES as readonly number[]).includes(n) ? n : 10));
}

/** Czy minął czas bezczynności (czysta funkcja – testowana). */
export function idleExpired(lastActivityMs: number, nowMs: number, minutes: number): boolean {
  return minutes > 0 && nowMs - lastActivityMs >= minutes * 60_000;
}

let locked = false;
let enabled = false;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());

export const appLock = {
  isLocked: () => locked,
  isEnabled: () => enabled,
  /** Wywoływane po sprawdzeniu, czy kod jest ustawiony (przy starcie i po zmianie w ustawieniach). */
  setEnabled(on: boolean) { enabled = on; if (!on) locked = false; emit(); },
  lock() { if (enabled && !locked) { locked = true; emit(); } },
  unlock() { if (locked) { locked = false; emit(); } },
  subscribe(f: () => void) { subs.add(f); return () => { subs.delete(f); }; },
};

let version = 0;
appLock.subscribe(() => { version += 1; });

export function useAppLock(): { locked: boolean; enabled: boolean } {
  useSyncExternalStore(appLock.subscribe, () => version);
  return { locked, enabled };
}
