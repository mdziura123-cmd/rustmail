// Sprawdzanie pisowni: słownik Hunspell (nspell) uruchamiany w osobnym wątku. Wszystko działa lokalnie.

export type SpellLang = "pl" | "en";

const CUSTOM_KEY = "rm.dict";

export function loadCustomWords(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(CUSTOM_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, 5000) : [];
  } catch { return []; }
}

function saveCustomWords(list: string[]) {
  try { localStorage.setItem(CUSTOM_KEY, JSON.stringify(list.slice(0, 5000))); } catch { /* pamięć niedostępna */ }
}

type Reply = { id: number; ok: boolean; error?: string; bad?: string[]; suggestions?: string[] };

let worker: Worker | null = null;
let loaded: SpellLang | null = null;
let ready: Promise<void> | null = null;
let nextId = 1;
const pending = new Map<number, (r: Reply) => void>();

function call(msg: Record<string, unknown>): Promise<Reply> {
  if (!worker) return Promise.reject(new Error("Brak wątku sprawdzania pisowni."));
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, (r) => (r.ok ? resolve(r) : reject(new Error(r.error ?? "Błąd sprawdzania pisowni."))));
    worker!.postMessage({ ...msg, id });
  });
}

async function fetchText(name: string): Promise<string> {
  const r = await fetch(new URL(`dict/${name}`, document.baseURI).toString());
  if (!r.ok) throw new Error("Brak pliku słownika w aplikacji.");
  return r.text();
}

/** Wczytuje słownik wybranego języka (przy zmianie języka wczytuje nowy). */
export function ensureSpell(lang: SpellLang): Promise<void> {
  if (ready && loaded === lang) return ready;
  loaded = lang;
  ready = (async () => {
    if (!worker) {
      worker = new Worker(new URL("./spell.worker.ts", import.meta.url), { type: "module" });
      worker.onmessage = (e: MessageEvent<Reply>) => {
        const f = pending.get(e.data.id);
        if (f) { pending.delete(e.data.id); f(e.data); }
      };
      worker.onerror = () => {
        for (const [, f] of pending) f({ id: 0, ok: false, error: "Wątek sprawdzania pisowni przestał działać." });
        pending.clear();
        worker = null;
        ready = null;
      };
    }
    const [aff, dic] = await Promise.all([fetchText(`${lang}.aff`), fetchText(`${lang}.dic`)]);
    await call({ type: "init", aff, dic, words: loadCustomWords() });
  })().catch((e) => { ready = null; loaded = null; throw e; });
  return ready;
}

export async function checkWords(words: string[]): Promise<string[]> {
  return (await call({ type: "check", words })).bad ?? [];
}

export async function suggestWord(word: string): Promise<string[]> {
  return (await call({ type: "suggest", word })).suggestions ?? [];
}

export async function addCustomWord(word: string): Promise<void> {
  const list = loadCustomWords();
  if (!list.includes(word)) saveCustomWords([...list, word]);
  await call({ type: "add", word });
}

/** Słowa do sprawdzenia: bez cytatów, adresów, skrótów wielkimi literami i nazw własnych w środku zdania. */
export function extractWords(html: string): string[] {
  let text = "";
  try {
    const doc = new DOMParser().parseFromString(html, "text/html");
    doc.querySelectorAll("blockquote,script,style,pre,code").forEach((n) => n.remove());
    doc.querySelectorAll("br,p,div,li").forEach((n) => n.append("\n"));
    text = doc.body?.textContent ?? "";
  } catch { return []; }
  text = text.replace(/https?:\/\/\S+|www\.\S+|[\w.+-]+@[\w-]+\.[\w.-]+/g, " ");
  const seen = new Set<string>();
  const out: string[] = [];
  const re = /\p{L}[\p{L}\p{M}'’]*/gu;
  for (const m of text.matchAll(re)) {
    const w = m[0].replace(/['’]+$/, "");
    if (w.length < 3 || seen.has(w)) continue;
    if (!/^\p{Script=Latin}+(?:['’]\p{Script=Latin}+)*$/u.test(w)) continue;
    if (w === w.toUpperCase()) continue; // skróty
    const startsUpper = w[0] !== w[0].toLowerCase();
    if (startsUpper) {
      const before = text.slice(0, m.index ?? 0).trimEnd();
      const prev = before.slice(-1);
      const sentenceStart = before === "" || ".!?:".includes(prev) || /\n\s*$/.test(text.slice(0, m.index ?? 0));
      if (!sentenceStart) continue; // nazwa własna w środku zdania
    }
    seen.add(w);
    out.push(w);
    if (out.length >= 400) break;
  }
  return out;
}
