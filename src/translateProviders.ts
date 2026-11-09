export type Provider = "mymemory" | "deepl" | "google" | "microsoft";

export interface ProviderInfo {
  id: Provider;
  name: string;
  needsKey: boolean;
  hint: string;
  keyHint?: string;
}

export const PROVIDERS: ProviderInfo[] = [
  { id: "mymemory", name: "MyMemory", needsKey: false,
    hint: "Darmowa, bez konta i klucza. Jakość przeciętna, dzienny limit znaków." },
  { id: "deepl", name: "DeepL", needsKey: true,
    hint: "Zwykle najlepsza jakość. Plan DeepL API Free: około 500 000 znaków miesięcznie (konto na deepl.com).",
    keyHint: "Klucz DeepL API (darmowy kończy się na :fx)" },
  { id: "google", name: "Google Cloud Translation", needsKey: true,
    hint: "Klucz API z Google Cloud (wymaga projektu z włączoną usługą Cloud Translation API; po darmowym limicie płatne).",
    keyHint: "Klucz API Google Cloud" },
  { id: "microsoft", name: "Microsoft Translator (Azure)", needsKey: true,
    hint: "Zasób Translator w Azure; plan darmowy ma około 2 mln znaków miesięcznie. Potrzebny klucz i region zasobu.",
    keyHint: "Klucz Azure Translator" },
];

const KEY = "rm.translate.provider";

export function getProvider(): Provider {
  try {
    const v = localStorage.getItem(KEY);
    return PROVIDERS.some((p) => p.id === v) ? (v as Provider) : "mymemory";
  } catch { return "mymemory"; }
}

export function setProvider(p: Provider) {
  try { localStorage.setItem(KEY, p); } catch { /* pamięć niedostępna */ }
}
