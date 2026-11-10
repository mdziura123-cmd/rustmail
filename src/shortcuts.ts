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

type Rebindable = Exclude<Cmd, "help" | "escape">;

/** Polecenia, którym użytkownik może przypisać własny klawisz. `def` to klawisz domyślny. */
export const BINDINGS: { cmd: Rebindable; what: string; def: string }[] = [
  { cmd: "next", what: "Następna konwersacja", def: "j" },
  { cmd: "prev", what: "Poprzednia konwersacja", def: "k" },
  { cmd: "select", what: "Zaznacz / odznacz konwersację na liście", def: "x" },
  { cmd: "archive", what: "Archiwizuj (zaznaczone lub otwartą)", def: "e" },
  { cmd: "trash", what: "Usuń (przenieś do kosza)", def: "#" },
  { cmd: "spam", what: "Oznacz jako spam", def: "!" },
  { cmd: "reply", what: "Odpowiedz", def: "r" },
  { cmd: "replyAll", what: "Odpowiedz wszystkim", def: "a" },
  { cmd: "forward", what: "Przekaż dalej", def: "f" },
  { cmd: "compose", what: "Nowa wiadomość", def: "c" },
  { cmd: "star", what: "Gwiazdka", def: "s" },
  { cmd: "read", what: "Oznacz jako przeczytane", def: "I" },
  { cmd: "unread", what: "Oznacz jako nieprzeczytane", def: "U" },
  { cmd: "search", what: "Szukaj", def: "/" },
  { cmd: "prio1", what: "Flaga: bardzo ważne", def: "1" },
  { cmd: "prio2", what: "Flaga: ważne", def: "2" },
  { cmd: "prio3", what: "Flaga: mniej ważne", def: "3" },
  { cmd: "prio0", what: "Usuń flagę", def: "0" },
];

/** Klawisze, których nie można zmienić ani przypisać do innych poleceń. */
const FIXED: Record<string, Cmd> = { "?": "help", Escape: "escape", Delete: "trash" };

let keys: Record<string, string> = Object.fromEntries(BINDINGS.map((b) => [b.cmd, b.def]));
let map: Record<string, Cmd> = {};
let version = 0;
const listeners = new Set<() => void>();

function rebuild() {
  map = { ...FIXED };
  for (const b of BINDINGS) map[keys[b.cmd]] = b.cmd;
  version++;
  listeners.forEach((f) => f());
}
rebuild();

export const keysVersion = () => version;
export function subscribeKeys(f: () => void): () => void { listeners.add(f); return () => { listeners.delete(f); }; }

/** Aktualny klawisz polecenia. */
export function keyOf(cmd: Rebindable): string { return keys[cmd]; }

/** Nazwa klawisza do wyświetlenia (wielka litera = z Shiftem). */
export function keyLabel(k: string): string {
  return k.length === 1 && k !== k.toLowerCase() ? "Shift+" + k.toLowerCase() : k === " " ? "Spacja" : k;
}

/** Czy klawisz nadaje się na skrót; zwraca komunikat błędu albo null. */
export function validateKey(key: string, cmd: Rebindable): string | null {
  const ok = (key.length === 1 && key !== " ") || /^F([1-9]|1[0-2])$/.test(key);
  if (!ok) return "Ten klawisz nie może być skrótem.";
  if (Object.prototype.hasOwnProperty.call(FIXED, key)) return "Ten klawisz jest zarezerwowany (" + keyLabel(key) + ").";
  const other = BINDINGS.find((b) => b.cmd !== cmd && keys[b.cmd] === key);
  if (other) return "Ten klawisz jest już użyty: " + other.what + ".";
  return null;
}

/** Ustawia klawisz polecenia (po walidacji); zwraca komunikat błędu albo null. */
export function setKey(cmd: Rebindable, key: string): string | null {
  const err = validateKey(key, cmd);
  if (err) return err;
  keys = { ...keys, [cmd]: key };
  rebuild();
  return null;
}

export function resetKeys() {
  keys = Object.fromEntries(BINDINGS.map((b) => [b.cmd, b.def]));
  rebuild();
}

/** Zapisane przez użytkownika różnice względem domyślnych (do zapisania w pamięci przeglądarki). */
export function customKeys(): Record<string, string> {
  return Object.fromEntries(BINDINGS.filter((b) => keys[b.cmd] !== b.def).map((b) => [b.cmd, keys[b.cmd]]));
}

/** Wczytuje zapisane skróty. Jeśli zapis jest błędny albo ma powtórzone klawisze, wracają wszystkie domyślne. */
export function applyCustomKeys(saved: unknown) {
  resetKeys();
  if (!saved || typeof saved !== "object") return;
  const want: Record<string, string> = { ...keys };
  for (const b of BINDINGS) {
    const v = (saved as Record<string, unknown>)[b.cmd];
    if (typeof v === "string") want[b.cmd] = v;
  }
  const used = new Set<string>();
  for (const b of BINDINGS) {
    const v = want[b.cmd];
    const fine = ((v.length === 1 && v !== " ") || /^F([1-9]|1[0-2])$/.test(v)) && !Object.prototype.hasOwnProperty.call(FIXED, v) && !used.has(v);
    if (!fine) return;
    used.add(v);
  }
  keys = want;
  rebuild();
}

/** Polecenie dla naciśniętego klawisza albo null. Kombinacje z Ctrl/Alt/Meta są zawsze pomijane. */
export function commandFor(k: KeyInfo): Cmd | null {
  if (k.ctrlKey || k.metaKey || k.altKey) return null;
  return Object.prototype.hasOwnProperty.call(map, k.key) ? map[k.key] : null;
}

/** Czy fokus jest w polu, do którego użytkownik wpisuje tekst (wtedy skróty są wyłączone). */
export function isTypingTarget(t: { tagName?: string; isContentEditable?: boolean } | null): boolean {
  if (!t) return false;
  if (t.isContentEditable) return true;
  const tag = (t.tagName ?? "").toUpperCase();
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/** Lista skrótów do okien pomocy (uwzględnia zmiany użytkownika). */
export function shortcutHelp(): { keys: string; what: string }[] {
  const b = (cmd: Rebindable) => BINDINGS.find((x) => x.cmd === cmd)!;
  const row = (cmd: Rebindable, extra = "") => ({ keys: keyLabel(keys[cmd]) + extra, what: b(cmd).what });
  return [
    { keys: keyLabel(keys.next) + " / " + keyLabel(keys.prev), what: "następna / poprzednia konwersacja" },
    { keys: "Ctrl+klik", what: "dodaj / odejmij konwersację od zaznaczenia" },
    { keys: "Shift+klik", what: "zaznacz zakres konwersacji" },
    { keys: "Ctrl+A", what: "zaznacz wszystkie konwersacje na liście" },
    row("select"), row("archive"), row("trash", " lub Delete"), row("spam"),
    { keys: [keys.reply, keys.replyAll, keys.forward].map(keyLabel).join(" / "), what: "odpowiedz / odpowiedz wszystkim / przekaż dalej" },
    row("compose"), row("star"),
    { keys: keyLabel(keys.read) + " / " + keyLabel(keys.unread), what: "oznacz jako przeczytane / nieprzeczytane" },
    { keys: [keys.prio1, keys.prio2, keys.prio3, keys.prio0].map(keyLabel).join(" / "), what: "flaga: bardzo ważne / ważne / mniej ważne / usuń flagę" },
    row("search"),
    { keys: "Esc", what: "odznacz zaznaczone" },
    { keys: "?", what: "ta pomoc" },
  ];
}
