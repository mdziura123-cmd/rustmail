/** Sprawdzanie adresów e-mail wpisanych w polach Do / DW / UDW (czysta logika). */

export const validEmail = (e: string): boolean => {
  if (e.length < 5 || e.length > 254) return false;
  const at = e.indexOf("@");
  if (at < 1 || at !== e.lastIndexOf("@")) return false;
  const local = e.slice(0, at), domain = e.slice(at + 1);
  if (local.length > 64 || !domain.includes(".") || domain.includes("..")) return false;
  if (/^[.-]|[.-]$/.test(domain)) return false;
  return !/[\s<>,;"'()[\]\\]/.test(e) && !/[\u0000-\u001f]/.test(e);
};

/** Dzieli pole na pojedyncze adresy (przecinek lub średnik poza cudzysłowem i nawiasami ostrymi). */
export function splitField(s: string): string[] {
  const out: string[] = [];
  let cur = "", q = false, a = false;
  for (const c of s) {
    if (c === '"') { q = !q; cur += c; }
    else if (c === "<" && !q) { a = true; cur += c; }
    else if (c === ">" && !q) { a = false; cur += c; }
    else if ((c === "," || c === ";") && !q && !a) { out.push(cur); cur = ""; }
    else cur += c;
  }
  out.push(cur);
  return out.map((x) => x.trim()).filter(Boolean);
}

/** Adres z jednego wpisu: „Jan <jan@x.pl>” → jan@x.pl, „jan@x.pl” → jan@x.pl. */
export function emailOf(part: string): string {
  const a = part.lastIndexOf("<"), b = part.lastIndexOf(">");
  return (a >= 0 && b > a ? part.slice(a + 1, b) : part).trim().toLowerCase();
}

export type FieldCheck = { state: "empty" } | { state: "ok"; count: number } | { state: "bad"; bad: string[] };

/** Ocena całego pola: wszystkie adresy poprawne (zielona kropka) albo lista błędnych. */
export function checkField(s: string): FieldCheck {
  const parts = splitField(s);
  if (parts.length === 0) return { state: "empty" };
  const bad = parts.filter((p) => !validEmail(emailOf(p)));
  return bad.length === 0 ? { state: "ok", count: parts.length } : { state: "bad", bad };
}
