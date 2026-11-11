// Prosty dziennik zdarzeń w pamięci (ostatnie 200 wpisów): pomaga ustalić, dlaczego coś nie działa
// (np. brak sieci przy sprawdzaniu poczty). Nic nie jest zapisywane na dysku ani wysyłane.
import { useSyncExternalStore } from "react";

export interface LogEntry { t: number; kind: string; text: string }

const MAX = 200;
let entries: LogEntry[] = [];
const subs = new Set<() => void>();

export function logEvent(kind: string, text: unknown) {
  const s = (text instanceof Error ? text.message : String(text)).replace(/\s+/g, " ").slice(0, 300);
  entries = [{ t: Date.now(), kind, text: s }, ...entries].slice(0, MAX);
  subs.forEach((f) => f());
}

export function clearLog() {
  entries = [];
  subs.forEach((f) => f());
}

export function useLog(): LogEntry[] {
  return useSyncExternalStore(
    (f) => { subs.add(f); return () => { subs.delete(f); }; },
    () => entries,
  );
}
