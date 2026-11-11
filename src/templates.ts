// Szablony odpowiedzi – zapisywane lokalnie (i w kopii zapasowej ustawień).
export interface Template { name: string; text: string }

export const MAX_TEMPLATES = 15;
export const MAX_NAME = 40;
export const MAX_TEXT = 1000;
const KEY = "rm.templates";

export function loadTemplates(): Template[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (!Array.isArray(v)) return [];
    return v
      .filter((t): t is Template => !!t && typeof t.name === "string" && typeof t.text === "string")
      .slice(0, MAX_TEMPLATES)
      .map((t) => ({ name: t.name.slice(0, MAX_NAME), text: t.text.slice(0, MAX_TEXT) }));
  } catch {
    return [];
  }
}

export function saveTemplates(list: Template[]): void {
  try { localStorage.setItem(KEY, JSON.stringify(list.slice(0, MAX_TEMPLATES))); } catch { /* pamięć niedostępna */ }
}
