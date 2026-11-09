// Sprawdzanie pisowni w osobnym wątku (duży słownik nie może blokować pisania).
import nspell from "nspell";

type Msg =
  | { type: "init"; id: number; aff: string; dic: string; words: string[] }
  | { type: "check"; id: number; words: string[] }
  | { type: "suggest"; id: number; word: string }
  | { type: "add"; id: number; word: string };

const ctx = self as unknown as {
  onmessage: ((e: MessageEvent<Msg>) => void) | null;
  postMessage: (m: unknown) => void;
};

let spell: ReturnType<typeof nspell> | null = null;

ctx.onmessage = (e) => {
  const m = e.data;
  try {
    if (m.type === "init") {
      spell = nspell(m.aff, m.dic);
      for (const w of m.words) spell.add(w);
      ctx.postMessage({ id: m.id, ok: true });
    } else if (!spell) {
      ctx.postMessage({ id: m.id, ok: false, error: "Słownik nie jest wczytany." });
    } else if (m.type === "check") {
      const bad = m.words.filter((w) => !spell!.correct(w) && !spell!.correct(w.toLowerCase()));
      ctx.postMessage({ id: m.id, ok: true, bad });
    } else if (m.type === "suggest") {
      ctx.postMessage({ id: m.id, ok: true, suggestions: spell.suggest(m.word).slice(0, 5) });
    } else {
      spell.add(m.word);
      ctx.postMessage({ id: m.id, ok: true });
    }
  } catch (err) {
    ctx.postMessage({ id: m.id, ok: false, error: String(err) });
  }
};
