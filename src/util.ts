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
/**
 * Wycina kolorowe emoji. Silnik WebKit w niektórych systemach (np. Bazzite) wysypuje się przy rysowaniu
 * kolorowych glifów COLRv1 (błąd w bibliotece Skia), co zawieszało foldery Spam i Nieprzeczytane.
 */
export function stripEmoji(s: string): string {
  return s.replace(
    /[\u{1F000}-\u{1FAFF}\u{2600}-\u{27BF}\u{2B00}-\u{2BFF}\u{231A}-\u{231B}\u{23E9}-\u{23F3}\u{23F8}-\u{23FA}\u{FE0E}\u{FE0F}\u{20E3}\u{E0020}-\u{E007F}]/gu,
    "",
  );
}

export function cleanDisplay(s: string | null | undefined, max = 200): string {
  if (!s) return "";
  let t = stripEmoji(s)
    .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
    .replace(/[­؜᠎​-‏‪-‮⁠-⁯﻿￹-￻]/g, "")
    .replace(/(\p{M}{2})\p{M}+/gu, "$1")
    .replace(/\s+/g, " ")
    .trim();
  if (t.length > max) t = t.slice(0, max - 1) + "…";
  return t;
}
