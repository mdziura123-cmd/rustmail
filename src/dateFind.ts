// Prosty wykrywacz terminu w treści wiadomości (do przycisku „Do kalendarza”).
const MONTHS = ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"];

const pad = (n: number) => String(n).padStart(2, "0");

function valid(y: number, m: number, d: number): boolean {
  if (y < 2000 || y > 2100 || m < 1 || m > 12 || d < 1 || d > 31) return false;
  const t = new Date(y, m - 1, d);
  return t.getFullYear() === y && t.getMonth() === m - 1 && t.getDate() === d;
}

export interface FoundDate { date: string; time?: string; found: boolean }

/** Szuka pierwszej daty (RRRR-MM-DD, DD.MM.RRRR, DD/MM/RRRR, „5 marca 2026”) i godziny GG:MM po niej. */
export function findDate(text: string, now = new Date()): FoundDate {
  const t = text.slice(0, 20000);
  const hits: { idx: number; len: number; y: number; m: number; d: number }[] = [];
  for (const m of t.matchAll(/\b(20\d{2})-(\d{2})-(\d{2})\b/g)) hits.push({ idx: m.index ?? 0, len: m[0].length, y: +m[1], m: +m[2], d: +m[3] });
  for (const m of t.matchAll(/\b(\d{1,2})[./](\d{1,2})[./](20\d{2})\b/g)) hits.push({ idx: m.index ?? 0, len: m[0].length, y: +m[3], m: +m[2], d: +m[1] });
  const re = new RegExp(`\\b(\\d{1,2})\\s+(${MONTHS.join("|")})(?:\\s+(20\\d{2}))?`, "gi");
  for (const m of t.matchAll(re)) {
    const mi = MONTHS.indexOf(m[2].toLowerCase()) + 1;
    let y = m[3] ? +m[3] : now.getFullYear();
    // bez roku: jeśli data już minęła o ponad miesiąc, zakładamy przyszły rok
    if (!m[3] && valid(y, mi, +m[1]) && new Date(y, mi - 1, +m[1]).getTime() < now.getTime() - 31 * 86_400_000) y += 1;
    hits.push({ idx: m.index ?? 0, len: m[0].length, y, m: mi, d: +m[1] });
  }
  const best = hits.filter((h) => valid(h.y, h.m, h.d)).sort((a, b) => a.idx - b.idx)[0];
  if (!best) {
    const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
    return { date: `${tomorrow.getFullYear()}-${pad(tomorrow.getMonth() + 1)}-${pad(tomorrow.getDate())}`, found: false };
  }
  const after = t.slice(best.idx + best.len, best.idx + best.len + 80);
  const tm = after.match(/\b([01]?\d|2[0-3]):([0-5]\d)\b/) ?? t.match(/\b([01]?\d|2[0-3]):([0-5]\d)\b/);
  return { date: `${best.y}-${pad(best.m)}-${pad(best.d)}`, time: tm ? `${pad(+tm[1])}:${tm[2]}` : undefined, found: true };
}
