import { useEffect, useMemo, useRef, useState } from "react";
import { api, errText } from "./api";
import { analyzeLink, type LinkAnalysis } from "./linkCheck";
import { sanitizeMail } from "./mailHtml";
import { splitQuoted } from "./quoteTrim";
import type { MessageView } from "./types";

const TRUST_KEY = "rm.trusted";

function loadTrusted(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(TRUST_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
  } catch { return []; }
}
function saveTrusted(list: string[]) {
  try { localStorage.setItem(TRUST_KEY, JSON.stringify(list.slice(0, 500))); } catch { /* pamięć niedostępna */ }
}

interface Pending extends LinkAnalysis {
  href: string;
}

/** Treść wiadomości: oczyszczony HTML (obrazy zablokowane do czasu zgody) albo zwykły tekst. */
export function MessageBody({ m, strict }: { m: MessageView; strict: boolean }) {
  const sender = m.fromEmail.toLowerCase();
  const [trusted, setTrusted] = useState<string[]>(loadTrusted);
  const isTrusted = !strict && !m.mine && trusted.includes(sender);
  const [showImages, setShowImages] = useState(false);
  const [asText, setAsText] = useState(!m.html);
  const [showQuote, setShowQuote] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const allowImages = showImages || isTrusted;

  const clean = useMemo(() => (m.html ? sanitizeMail(m.html, allowImages, !showQuote) : null), [m.html, allowImages, showQuote]);
  const split = useMemo(() => splitQuoted(m.body), [m.body]);

  // Obrazy pobiera backend (tylko https, publiczne adresy, limit rozmiaru) po zgodzie użytkownika.
  useEffect(() => {
    if (!allowImages || asText || !box.current) return;
    const imgs = Array.from(box.current.querySelectorAll<HTMLImageElement>("img[data-rm-src]"));
    let cancelled = false;
    let next = 0;
    const worker = async () => {
      while (!cancelled) {
        const img = imgs[next++];
        if (!img) return;
        try {
          const uri = await api.fetchImage(img.getAttribute("data-rm-src") ?? "");
          if (!cancelled) { img.src = uri; img.removeAttribute("data-rm-src"); }
        } catch {
          if (!cancelled) {
            const ph = document.createElement("span");
            ph.className = "img-blocked";
            ph.textContent = "[obraz niedostępny]";
            img.replaceWith(ph);
          }
        }
      }
    };
    void Promise.all([worker(), worker(), worker()]);
    return () => { cancelled = true; };
  }, [allowImages, asText, clean]);

  const onActivate = (target: EventTarget | null) => {
    const a = (target as HTMLElement | null)?.closest?.("a.mail-link") as HTMLElement | null;
    if (!a) return false;
    const href = a.getAttribute("data-href") ?? "";
    setNote(null);
    const info = analyzeLink(a.textContent ?? "", href);
    if (strict) info.warnings.push("Wiadomość z tym linkiem została oceniona jako wysokiego ryzyka (phishing).");
    setPending({ href, ...info });
    return true;
  };

  const open = async () => {
    if (!pending) return;
    try { await api.openUrl(pending.href); setPending(null); }
    catch (e) { setNote(errText(e)); }
  };
  const copy = async () => {
    if (!pending) return;
    try { await navigator.clipboard.writeText(pending.href); setNote("Adres skopiowany."); }
    catch { setNote("Nie udało się skopiować – zaznacz adres i skopiuj ręcznie."); }
  };
  const trustSender = () => {
    const next = Array.from(new Set([...trusted, sender]));
    setTrusted(next);
    saveTrusted(next);
  };

  if (asText || !clean) {
    return (
      <>
        {m.html && <button className="linklike plain textmode" onClick={() => setAsText(false)}>Pokaż sformatowaną wersję</button>}
        <p className="msg">{(showQuote ? m.body : split.main) || "(brak treści tekstowej)"}</p>
        {split.quoted && (
          <button className="linklike plain quote-toggle" onClick={() => setShowQuote((v) => !v)}>
            {showQuote ? "Ukryj cytowany tekst" : "Pokaż cytowany tekst"}
          </button>
        )}
      </>
    );
  }

  return (
    <>
      <div className="mail-tools">
        <button className="linklike plain" onClick={() => setAsText(true)}>Pokaż jako zwykły tekst</button>
      </div>
      {clean.hadQuote && (
        <button className="linklike plain quote-toggle" onClick={() => setShowQuote((v) => !v)}>
          {showQuote ? "Ukryj cytowany tekst" : "Pokaż cytowany tekst"}
        </button>
      )}
      {clean.remoteImages > 0 && !allowImages && (
        <div className="img-bar" role="status">
          <span>Zablokowano zdalne obrazy ({clean.remoteImages}) – mogą ujawniać, że otworzyłeś wiadomość.</span>
          <button className="small" onClick={() => setShowImages(true)}>Pokaż obrazy</button>
          {!m.mine && !strict && <button className="small" onClick={() => { trustSender(); }}>Zawsze ufaj {m.fromEmail}</button>}
        </div>
      )}
      {isTrusted && clean.remoteImages > 0 && (
        <p className="note">
          Obrazy są pobierane, bo ufasz nadawcy.{" "}
          <button className="linklike plain" onClick={() => { const n = trusted.filter((x) => x !== sender); setTrusted(n); saveTrusted(n); }}>
            Cofnij zaufanie
          </button>
        </p>
      )}
      <div ref={box} className="mail-html" dangerouslySetInnerHTML={{ __html: clean.html }}
        onClick={(e) => { if (onActivate(e.target)) e.preventDefault(); }}
        onKeyDown={(e) => { if (e.key === "Enter" && onActivate(e.target)) e.preventDefault(); }} />
      {pending && (
        <div className={"link-bar" + (pending.warnings.length ? " danger" : "")} role="alertdialog" aria-label="Potwierdzenie otwarcia linku">
          <div>Ten link prowadzi do: <strong className="url">{pending.href.length > 300 ? pending.href.slice(0, 300) + "…" : pending.href}</strong></div>
          {pending.warnings.map((w) => <div key={w} className="warn-line">⚠ {w}</div>)}
          {note && <div className="note">{note}</div>}
          <div className="link-actions">
            <button className="small solid" onClick={() => void open()}>{strict ? "Rozumiem ryzyko – otwórz" : "Otwórz w przeglądarce"}</button>
            <button className="small" onClick={() => void copy()}>Kopiuj adres</button>
            <button className="small" onClick={() => { setPending(null); setNote(null); }}>Anuluj</button>
          </div>
        </div>
      )}
    </>
  );
}
