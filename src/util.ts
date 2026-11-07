export function fmtDate(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString("pl-PL", { hour: "2-digit", minute: "2-digit" });
  }
  return d.toLocaleDateString("pl-PL", {
    day: "2-digit",
    month: "2-digit",
    year: d.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}

export function fmtFull(ms: number): string {
  if (!ms) return "";
  return new Date(ms).toLocaleString("pl-PL", {
    day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
  });
}

export function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/**
 * Tekst do wyświetlenia w nagłówkach i listach (temat, nadawca, fragment). Usuwa znaki, którymi spam
 * myli lub psuje wyświetlanie: sterujące, niewidoczne, zmieniające kierunek tekstu (podszywanie się)
 * oraz długie ciągi znaków łączących ("zalgo"), i przycina długość.
 */
export function cleanDisplay(s: string | null | undefined, max = 200): string {
  if (!s) return "";
  let t = s
    .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
    .replace(/[­؜᠎​-‏‪-‮⁠-⁯﻿￹-￻]/g, "")
    .replace(/(\p{M}{2})\p{M}+/gu, "$1")
    .replace(/\s+/g, " ")
    .trim();
  if (t.length > max) t = t.slice(0, max - 1) + "…";
  return t;
}
