import { useEffect, useState } from "react";
import { api, errText } from "./api";
import { isNewSender, useKnownSenders } from "./knownSenders";
import type { MailSource, MessageView, Unsubscribe } from "./types";

interface Props {
  m: MessageView;
  /** Wczytanie źródła tej wiadomości (Gmail albo IMAP). */
  loadSource: () => Promise<MailSource>;
  /** Wypisanie przez e-mail: otwiera nową wiadomość (nic nie wysyła się samo). */
  onMail: (to: string, subject: string, body: string) => void;
  /** Kto weryfikował nadawcę: Gmail (DMARC) albo Poczta WP (podpis DKIM). */
  verifier?: "gmail" | "wp";
}

/** Pasek pod nagłówkiem wiadomości: weryfikacja nadawcy, „nowy nadawca”, wypisanie z listy, źródło. */
export function MessageMeta({ m, loadSource, onMail, verifier = "gmail" }: Props) {
  const book = useKnownSenders();
  const [unsub, setUnsub] = useState<Unsubscribe | null>(null);
  const [source, setSource] = useState(false);
  if (m.mine) return null;
  const isNew = isNewSender(m.fromEmail, m.mine, book);
  const domain = m.fromEmail.split("@")[1] ?? "";
  return (
    <div className="msg-meta">
      {m.auth === "verified" && (
        <span className="chip ok" title={verifier === "wp"
          ? `Poczta WP potwierdziła podpis cyfrowy (DKIM) domeny ${domain} – wiadomość naprawdę pochodzi z tej domeny. To nie oznacza, że treść jest bezpieczna.`
          : `Gmail potwierdził (DMARC), że wiadomość naprawdę wysłano z domeny ${domain}. To nie oznacza, że treść jest bezpieczna.`}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6zM9 12l2 2 4-4" /></svg>
          Nadawca zweryfikowany
        </span>
      )}
      {m.auth === "failed" && (
        <span className="chip bad" title={verifier === "wp"
          ? `Poczta WP uznała podpis cyfrowy (DKIM) za nieprawidłowy. Adres nadawcy mógł zostać podrobiony.`
          : `Gmail nie potwierdził, że wiadomość wysłano z domeny ${domain}. Adres nadawcy mógł zostać podrobiony.`}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6zM12 8v5M12 16h.01" /></svg>
          Nadawca niezweryfikowany
        </span>
      )}
      {isNew && (
        <span className="chip new" title="Tego adresu nie ma w Twojej książce adresowej – nie pisaliście ze sobą ostatnio. Zachowaj ostrożność przy linkach i załącznikach.">
          Nowy nadawca
        </span>
      )}
      <span className="meta-actions">
        {m.unsubscribe && (
          <button className="linklike plain" onClick={() => setUnsub(m.unsubscribe ?? null)}>Wypisz się</button>
        )}
        <button className="linklike plain" onClick={() => setSource(true)}>Źródło</button>
      </span>
      {unsub && (
        <UnsubDialog u={unsub} from={m.fromEmail} onClose={() => setUnsub(null)}
          onMail={(to, s, b) => { setUnsub(null); onMail(to, s, b); }} />
      )}
      {source && <SourceDialog load={loadSource} onClose={() => setSource(false)} />}
    </div>
  );
}

function UnsubDialog({ u, from, onClose, onMail }: { u: Unsubscribe; from: string; onClose: () => void; onMail: (to: string, s: string, b: string) => void }) {
  const [err, setErr] = useState<string | null>(null);
  const host = u.url ? (/^https:\/\/([^/?#]+)/i.exec(u.url)?.[1] ?? "") : "";
  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Wypisz się z listy">
      <div className="modal">
        <h3>Wypisać się z listy {from}?</h3>
        <p className="hint">
          Nadawca podał w wiadomości sposób wypisania się. Używaj go tylko przy newsletterach, które sam(a) kiedyś
          zamówiłeś(-aś). Przy podejrzanym spamie lepiej oznaczyć wiadomość jako spam – kliknięcie „wypisz” potwierdza
          oszustom, że adres jest aktywny.
        </p>
        {u.url && <p className="hint">Strona wypisania otworzy się w przeglądarce: <strong>{host}</strong></p>}
        {u.mailto && <p className="hint">Albo wyślij wiadomość na adres <strong>{u.mailto}</strong> (otworzy się okno nowej wiadomości).</p>}
        {err && <p className="error" role="alert">{err}</p>}
        <div className="modal-actions wrap">
          {u.url && (
            <button className="primary" onClick={() => { api.openUrl(u.url ?? "").then(onClose).catch((e) => setErr(errText(e))); }}>
              Otwórz stronę wypisania
            </button>
          )}
          {u.mailto && (
            <button className={u.url ? "" : "primary"} onClick={() => onMail(u.mailto ?? "", u.mailSubject || "unsubscribe", u.mailBody)}>
              Napisz wiadomość
            </button>
          )}
          <button onClick={onClose}>Anuluj</button>
        </div>
      </div>
    </div>
  );
}

function SourceDialog({ load, onClose }: { load: () => Promise<MailSource>; onClose: () => void }) {
  const [src, setSrc] = useState<MailSource | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [full, setFull] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    load().then((s) => { if (live) setSrc(s); }).catch((e) => { if (live) setErr(errText(e)); });
    return () => { live = false; };
  // Wczytujemy raz po otwarciu okna.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const text = src ? (full ? src.full : src.headers) : "";
  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Źródło wiadomości"
      onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}>
      <div className="modal source-modal">
        <h3>Źródło wiadomości</h3>
        <p className="hint">
          Nagłówki pokazują, przez jakie serwery przeszła wiadomość i jakie wyniki dała weryfikacja (Authentication-Results).
          Treść jest pokazana jako zwykły tekst – nic tu się nie uruchamia ani nie pobiera.
        </p>
        <div className="modal-actions wrap">
          <button className={!full ? "primary" : ""} aria-pressed={!full} onClick={() => setFull(false)}>Nagłówki</button>
          <button className={full ? "primary" : ""} aria-pressed={full} onClick={() => setFull(true)}>Całość</button>
          <button disabled={!src} onClick={() => { void navigator.clipboard.writeText(text).then(() => setNote("Skopiowano.")).catch(() => setNote("Nie udało się skopiować.")); }}>Kopiuj</button>
          <button onClick={onClose}>Zamknij</button>
        </div>
        {src && full && src.truncated && <p className="hint">Wiadomość ma {Math.round(src.size / 1024)} KB – pokazano tylko początek.</p>}
        {note && <p className="hint" role="status">{note}</p>}
        {err && <p className="error" role="alert">{err}</p>}
        {!src && !err && <p className="hint">Wczytywanie…</p>}
        {src && <pre className="source-box" tabIndex={0}>{text}</pre>}
      </div>
    </div>
  );
}
