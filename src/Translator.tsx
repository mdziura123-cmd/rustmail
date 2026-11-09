import { useMemo, useState } from "react";
import { api, errText } from "./api";
import { getProvider, PROVIDERS } from "./translateProviders";
import type { MessageView } from "./types";

const okKey = (provider: string) => "rm.translate.ok." + provider;

function readOk(provider: string): boolean {
  try { return localStorage.getItem(okKey(provider)) === "1"; } catch { return false; }
}
function saveOk(provider: string) {
  try { localStorage.setItem(okKey(provider), "1"); } catch { /* pamięć niedostępna */ }
}

/** Tekst wiadomości do tłumaczenia: wersja tekstowa albo tekst wyciągnięty z HTML (bez wykonywania czegokolwiek). */
function plainText(m: MessageView): string {
  if (m.body.trim()) return m.body;
  if (!m.html) return "";
  try {
    const doc = new DOMParser().parseFromString(m.html, "text/html");
    doc.querySelectorAll("script,style,head").forEach((n) => n.remove());
    return (doc.body?.textContent ?? "").replace(/[ \t]+/g, " ").replace(/\n{3,}/g, "\n\n").trim();
  } catch { return ""; }
}

/** Czy tekst wygląda na niepolski (brak polskich liter, dość długi). */
function looksForeign(text: string): boolean {
  const sample = text.slice(0, 1500);
  const letters = (sample.match(/\p{L}/gu) ?? []).length;
  if (letters < 25) return false;
  return !/[ąćęłńóśźżĄĆĘŁŃÓŚŹŻ]/.test(sample);
}

/** Przycisk „Przetłumacz na polski” (angielski → polski) nad treścią wiadomości. */
export function Translator({ m }: { m: MessageView }) {
  const text = useMemo(() => plainText(m), [m]);
  const [ask, setAsk] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ translated: string; truncated: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!looksForeign(text)) return null;

  const run = async () => {
    setAsk(null);
    setBusy(true);
    setError(null);
    try {
      setResult(await api.translateText(text, getProvider()));
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const start = () => {
    const provider = getProvider();
    if (readOk(provider)) void run(); else setAsk(provider);
  };

  return (
    <div className="translator">
      {!result && !ask && (
        <button className="small" disabled={busy} onClick={start}>
          {busy ? "Tłumaczenie…" : "Przetłumacz na polski"}
        </button>
      )}
      {ask && (
        <p className="warn-line" role="alert">
          Treść tej wiadomości zostanie wysłana do usługi tłumaczenia: {PROVIDERS.find((p) => p.id === ask)?.name ?? ask}. Możesz ją zmienić w Ustawieniach.
          Robimy to tylko po kliknięciu. Nie tłumacz wiadomości poufnych.{" "}
          <button className="small solid" onClick={() => { saveOk(ask); void run(); }}>Rozumiem, tłumacz</button>{" "}
          <button className="small" onClick={() => setAsk(null)}>Anuluj</button>
        </p>
      )}
      {error && <p className="error" role="alert">{error}</p>}
      {result && (
        <div className="translation" role="region" aria-label="Tłumaczenie">
          <div className="translation-head">
            <strong>Tłumaczenie na polski</strong>
            <span className="hint"> (automatyczne, może zawierać błędy)</span>
            <button className="small" onClick={() => setResult(null)}>Ukryj</button>
          </div>
          <p className="msg">{result.translated}</p>
          {result.truncated && <p className="hint">Wiadomość była długa – przetłumaczono tylko jej początek.</p>}
        </div>
      )}
    </div>
  );
}
