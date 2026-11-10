/** Skróty klawiszowe (jak w Gmailu). Czysta logika: łatwa do przetestowania, bez zależności od React. */

export type Cmd =
  | "next" | "prev" | "archive" | "trash" | "spam" | "reply" | "replyAll" | "forward"
  | "prio0" | "prio1" | "prio2" | "prio3" | "compose" | "star" | "read" | "unread" | "search" | "select" | "help" | "escape";

export interface KeyInfo {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

const MAP: Record<string, Cmd> = {
  j: "next", k: "prev", e: "archive", "#": "trash", Delete: "trash", "!": "spam", r: "reply", a: "replyAll", f: "forward",
  c: "compose", s: "star", I: "read", U: "unread", "/": "search", x: "select", "?": "help", "0": "prio0", "1": "prio1", "2": "prio2", "3": "prio3", Escape: "escape",
};

/** Polecenie dla naciśniętego klawisza albo null. Kombinacje z Ctrl/Alt/Meta są zawsze pomijane. */
export function commandFor(k: KeyInfo): Cmd | null {
  if (k.ctrlKey || k.metaKey || k.altKey) return null;
  return Object.prototype.hasOwnProperty.call(MAP, k.key) ? MAP[k.key] : null;
}

/** Czy fokus jest w polu, do którego użytkownik wpisuje tekst (wtedy skróty są wyłączone). */
export function isTypingTarget(t: { tagName?: string; isContentEditable?: boolean } | null): boolean {
  if (!t) return false;
  if (t.isContentEditable) return true;
  const tag = (t.tagName ?? "").toUpperCase();
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export const SHORTCUT_HELP: { keys: string; what: string }[] = [
  { keys: "j / k", what: "następna / poprzednia konwersacja" },
  { keys: "Ctrl+klik", what: "dodaj / odejmij konwersację od zaznaczenia" },
  { keys: "Shift+klik", what: "zaznacz zakres konwersacji" },
  { keys: "Ctrl+A", what: "zaznacz wszystkie konwersacje na liście" },
  { keys: "x", what: "zaznacz / odznacz konwersację na liście" },
  { keys: "e", what: "archiwizuj (zaznaczone lub otwartą)" },
  { keys: "Delete lub #", what: "usuń (przenieś do kosza)" },
  { keys: "!", what: "oznacz jako spam" },
  { keys: "r / a / f", what: "odpowiedz / odpowiedz wszystkim / przekaż dalej" },
  { keys: "c", what: "nowa wiadomość" },
  { keys: "s", what: "gwiazdka" },
  { keys: "1 / 2 / 3 / 0", what: "flaga: bardzo ważne / ważne / mniej ważne / usuń flagę" },
  { keys: "Shift+I / Shift+U", what: "oznacz jako przeczytane / nieprzeczytane" },
  { keys: "/", what: "szukaj" },
  { keys: "Esc", what: "odznacz zaznaczone" },
  { keys: "?", what: "ta pomoc" },
];
