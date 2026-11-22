// Adresy z lokalnej książki adresowej (uzupełnianej z wysłanych i odebranych). Służy do etykiety
// „Nowy nadawca”. Dane zostają na komputerze; lista jest odświeżana najwyżej co 10 minut.
import { useEffect, useState } from "react";
import { api } from "./api";

let cache: Set<string> | null = null;
let loadedAt = 0;
let pending: Promise<Set<string>> | null = null;

/** Poniżej tylu adresów książka jest jeszcze pusta lub niepełna i etykieta byłaby myląca. */
export const MIN_BOOK = 10;

function load(): Promise<Set<string>> {
  if (cache && Date.now() - loadedAt < 600_000) return Promise.resolve(cache);
  if (!pending) {
    pending = api.contactsList()
      .then((list) => {
        cache = new Set(list.map((c) => c.email.trim().toLowerCase()));
        loadedAt = Date.now();
        return cache;
      })
      .catch(() => cache ?? new Set<string>())
      .finally(() => { pending = null; });
  }
  return pending;
}

/** Czy pokazać etykietę „Nowy nadawca” (czysta funkcja – testowana). */
export function isNewSender(email: string, mine: boolean, book: Set<string> | null): boolean {
  const e = email.trim().toLowerCase();
  if (mine || !book || book.size < MIN_BOOK || !e.includes("@")) return false;
  return !book.has(e);
}

export function useKnownSenders(): Set<string> | null {
  const [book, setBook] = useState<Set<string> | null>(cache);
  useEffect(() => {
    let live = true;
    void load().then((b) => { if (live) setBook(b); });
    return () => { live = false; };
  }, []);
  return book;
}
