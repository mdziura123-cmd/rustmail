import { useEffect, useRef, useState } from "react";
import type { Account, ImapAccount } from "./types";

interface Props {
  gmail: Account[];
  imap: ImapAccount[];
  /** "gmail" | "imap:adres" */
  view: string;
  /** Adres aktualnie otwartego konta. */
  label: string;
  busy: boolean;
  onGmail: (email: string) => void;
  onImap: (email: string) => void;
}

const hue = (s: string) => { let h = 0; for (const c of s) h = (h * 31 + c.charCodeAt(0)) % 360; return h; };
const initial = (s: string) => (s.trim()[0] ?? "?").toUpperCase();

/** Okrągły przełącznik kont na pasku okna (jak awatar w Outlooku). Wybór konta przełącza je od razu.
 *  Dodawanie i wylogowanie kont jest w Ustawieniach (zakładka „Konta”). */
export function AccountSwitcher(p: Props) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const total = p.gmail.length + p.imap.length;

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (box.current && !box.current.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => { window.removeEventListener("mousedown", away); window.removeEventListener("keydown", esc); };
  }, [open]);

  const pick = (on: () => void) => { setOpen(false); on(); };
  const item = (active: boolean, email: string, kind: string, on: () => void, disabled = false) => (
    <button key={kind + email} role="menuitemradio" aria-checked={active} disabled={disabled}
      className={active ? "acc-item active" : "acc-item"} onClick={() => pick(on)}>
      <span className="av sm" style={{ ["--hue" as string]: hue(email) }} aria-hidden="true">{initial(email)}</span>
      <span className="acc-name">{email}</span>
      <span className="acc-kind">{kind}</span>
      {active && <span className="acc-check" aria-hidden="true">✓</span>}
    </button>
  );

  return (
    <div className="acc-avatar" ref={box}>
      <button className={"av-btn" + (open ? " open" : "")} aria-haspopup="menu" aria-expanded={open}
        title={`Aktywne konto: ${p.label}${total > 1 ? " – kliknij, aby przełączyć" : ""}`} aria-label={`Aktywne konto: ${p.label}. Przełącz konto`}
        onClick={() => setOpen((v) => !v)}>
        <span key={p.label} className="av" style={{ ["--hue" as string]: hue(p.label) }}>{initial(p.label)}</span>
        <span className="av-dot" aria-hidden="true" />
      </button>
      {open && (
        <div className="acc-pop" role="menu">
          <div className="acc-title">Przełącz konto</div>
          {p.gmail.map((a) => item(p.view === "gmail" && a.active, a.email, "Gmail", () => p.onGmail(a.email), p.busy))}
          {p.imap.map((a) => item(p.view === "imap:" + a.email, a.email, "IMAP", () => p.onImap(a.email)))}
        </div>
      )}
    </div>
  );
}
