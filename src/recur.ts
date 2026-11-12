import type { CalEvent } from "./types";

export interface Occurrence { event: CalEvent; startMs: number; endMs: number }

const MAX_STEPS = 4000;

/** Początek k-tego wystąpienia (k = 0 to pierwotny termin). Czas lokalny, godzina bez zmian po zmianie czasu. */
export function occurrenceStart(e: CalEvent, k: number): number {
  const s = new Date(e.startMs);
  const y = s.getFullYear(), m = s.getMonth(), d = s.getDate();
  const h = s.getHours(), mi = s.getMinutes();
  switch (e.repeat) {
    case "daily": return new Date(y, m, d + k, h, mi).getTime();
    case "weekly": return new Date(y, m, d + 7 * k, h, mi).getTime();
    case "monthly": {
      const last = new Date(y, m + k + 1, 0).getDate();
      return new Date(y, m + k, Math.min(d, last), h, mi).getTime();
    }
    case "yearly": {
      const last = new Date(y + k, m + 1, 0).getDate();
      return new Date(y + k, m, Math.min(d, last), h, mi).getTime();
    }
    default: return e.startMs;
  }
}

/** Wystąpienia wydarzenia nachodzące na przedział [fromMs, toMs). Zwykłe wydarzenie ma jedno. */
export function occurrences(e: CalEvent, fromMs: number, toMs: number): Occurrence[] {
  const dur = e.endMs - e.startMs;
  if (e.repeat === "none" || !e.repeat) {
    return e.startMs < toMs && e.endMs >= fromMs ? [{ event: e, startMs: e.startMs, endMs: e.endMs }] : [];
  }
  const out: Occurrence[] = [];
  // dolna granica k, żeby nie iterować od początku (szacunek z zapasem)
  const DAYMS = 86_400_000;
  let k = 0;
  const gap = fromMs - dur - e.startMs;
  if (gap > 0) {
    const per = e.repeat === "daily" ? DAYMS : e.repeat === "weekly" ? 7 * DAYMS : e.repeat === "monthly" ? 31 * DAYMS : 366 * DAYMS;
    k = Math.max(0, Math.floor(gap / per) - 1);
  }
  for (let n = 0; n < MAX_STEPS; n++, k++) {
    const st = occurrenceStart(e, k);
    if (st >= toMs) break;
    if (e.untilMs && st > e.untilMs) break;
    const en = st + dur;
    if (en >= fromMs) out.push({ event: e, startMs: st, endMs: en });
  }
  return out;
}

/** Najpóźniejsze wystąpienie, którego czas przypomnienia już minął (a samo wydarzenie nie jest starsze niż doba). */
export function dueOccurrence(e: CalEvent, nowMs: number): number | null {
  if (e.remindMin < 0) return null;
  const from = nowMs - 86_400_000;
  const to = nowMs + e.remindMin * 60_000 + 1;
  let best: number | null = null;
  for (const o of occurrences(e, from, to)) {
    if (o.startMs < from || o.startMs - e.remindMin * 60_000 > nowMs) continue;
    if (o.startMs > (e.reminded ?? 0) && (best === null || o.startMs > best)) best = o.startMs;
  }
  return best;
}
