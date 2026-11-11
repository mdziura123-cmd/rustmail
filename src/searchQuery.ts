/** Opcje wyszukiwania (nadawca, temat, daty, flaga) zamieniane na zapytanie dla Gmaila albo IMAP. Czysta logika. */
export type FlagFilter = "any" | "flagged" | "1" | "2" | "3" | "none";
export interface Filters { from: string; subject: string; dateFrom: string; dateTo: string; flag: FlagFilter }
export const EMPTY_FILTERS: Filters = { from: "", subject: "", dateFrom: "", dateTo: "", flag: "any" };

export const FLAG_OPTIONS: { id: FlagFilter; label: string }[] = [
  { id: "any", label: "Dowolna" },
  { id: "flagged", label: "Z dowolną flagą" },
  { id: "1", label: "Bardzo ważne" },
  { id: "2", label: "Ważne" },
  { id: "3", label: "Mniej ważne" },
  { id: "none", label: "Bez flagi" },
];

export const hasFilters = (f: Filters) => Boolean(f.from.trim() || f.subject.trim() || f.dateFrom || f.dateTo || f.flag !== "any");

const DATE = /^(\d{4})-(\d{2})-(\d{2})$/;
export function validDate(s: string): boolean {
  const m = DATE.exec(s);
  if (!m) return false;
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
  return d.getUTCFullYear() === +m[1] && d.getUTCMonth() === +m[2] - 1 && d.getUTCDate() === +m[3];
}
const nextDay = (s: string) => new Date(Date.parse(s + "T00:00:00Z") + 86_400_000).toISOString().slice(0, 10);
const clean = (v: string) => v.replace(/["\u0000-\u001f]/g, " ").replace(/\s+/g, " ").trim();

/** Zwraca komunikat błędu, gdy opcje są niepoprawne (np. data „do” przed datą „od”). */
export function filtersError(f: Filters): string | null {
  if (f.dateFrom && !validDate(f.dateFrom)) return "Data „od” jest niepoprawna.";
  if (f.dateTo && !validDate(f.dateTo)) return "Data „do” jest niepoprawna.";
  if (f.dateFrom && f.dateTo && f.dateTo < f.dateFrom) return "Data „do” jest wcześniejsza niż „od”.";
  return null;
}

/** Składa zapytanie: wpisany tekst + warunki z opcji. Data „do” obejmuje cały wskazany dzień. */
export function buildQuery(text: string, f: Filters, dialect: "gmail" | "imap"): string {
  const parts: string[] = [];
  if (text.trim()) parts.push(text.trim());
  const from = clean(f.from), subject = clean(f.subject);
  const gm = dialect === "gmail";
  if (from) parts.push(`${gm ? "from" : "od"}:"${from}"`);
  if (subject) parts.push(`${gm ? "subject" : "temat"}:"${subject}"`);
  if (f.dateFrom && validDate(f.dateFrom)) parts.push(gm ? `after:${f.dateFrom.replace(/-/g, "/")}` : `po:${f.dateFrom}`);
  if (f.dateTo && validDate(f.dateTo)) parts.push(gm ? `before:${nextDay(f.dateTo).replace(/-/g, "/")}` : `przed:${nextDay(f.dateTo)}`);
  return parts.join(" ");
}

/** Czy wiadomość o danym poziomie flagi (0 = brak) spełnia filtr flagi. */
export function flagMatches(level: number, f: FlagFilter): boolean {
  switch (f) {
    case "any": return true;
    case "flagged": return level > 0;
    case "none": return level === 0;
    default: return level === Number(f);
  }
}
