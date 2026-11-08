/** Motywy kolorystyczne. Wszystkie kolory są jednolite (bez przezroczystości i rozmycia). */

export type ThemeId =
  | "auto" | "light" | "dark" | "proton" | "proton-dark" | "blue" | "solarized-light" | "solarized-dark";

export type Mode = "light" | "dark";

export const THEMES: { id: ThemeId; label: string; mode: Mode | null }[] = [
  { id: "auto", label: "Systemowy (jasny/ciemny)", mode: null },
  { id: "light", label: "Jasny", mode: "light" },
  { id: "dark", label: "Ciemny", mode: "dark" },
  { id: "proton", label: "Proton – fiolet i biel", mode: "light" },
  { id: "proton-dark", label: "Proton ciemny", mode: "dark" },
  { id: "blue", label: "Błękit i biel", mode: "light" },
  { id: "solarized-light", label: "Solarized jasny", mode: "light" },
  { id: "solarized-dark", label: "Solarized ciemny", mode: "dark" },
];

export function isThemeId(s: string | null | undefined): s is ThemeId {
  return THEMES.some((t) => t.id === s);
}

/** Motyw faktycznie użyty na ekranie (dla „Systemowego” wybierany wg ustawienia systemu). */
export function resolveTheme(id: ThemeId, systemDark: boolean): { id: Exclude<ThemeId, "auto">; mode: Mode } {
  if (id === "auto") return systemDark ? { id: "dark", mode: "dark" } : { id: "light", mode: "light" };
  const t = THEMES.find((x) => x.id === id);
  return { id: id as Exclude<ThemeId, "auto">, mode: t?.mode ?? "light" };
}
