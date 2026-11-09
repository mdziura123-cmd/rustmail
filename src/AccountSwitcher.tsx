import type { Account, ImapAccount } from "./types";

interface Props {
  gmail: Account[];
  imap: ImapAccount[];
  /** "gmail" | "imap:adres" */
  view: string;
  label: string;
  busy: boolean;
  onGmail: (email: string) => void;
  onImap: (email: string) => void;
}

/** Przełącznik kont. Dodawanie i wylogowanie kont jest w Ustawieniach (zakładka „Konta”). */
export function AccountSwitcher(p: Props) {
  const close = (e: React.MouseEvent<HTMLElement>) => {
    const d = e.currentTarget.closest("details");
    if (d) d.removeAttribute("open");
  };
  const total = p.gmail.length + p.imap.length;
  const item = (active: boolean, text: string, on: () => void, key: string, disabled = false) => (
    <button key={key} role="menuitemradio" aria-checked={active} disabled={disabled}
      className={active ? "acc-item active" : "acc-item"} onClick={(e) => { close(e); on(); }}>
      {active ? "✓ " : ""}{text}
    </button>
  );
  return (
    <details className="accounts">
      <summary title="Konta">
        <span className="acc-email">{p.label}</span>
        {total > 1 && <span className="acc-count">{total}</span>}
      </summary>
      <div className="acc-pop" role="menu">
        <div className="acc-title">Zalogowane konta</div>
        {p.gmail.map((a) => item(p.view === "gmail" && a.active, `${a.email} (Gmail)`, () => p.onGmail(a.email), "g" + a.email, p.busy))}
        {p.imap.map((a) => item(p.view === "imap:" + a.email, `${a.email} (IMAP)`, () => p.onImap(a.email), "i" + a.email))}
      </div>
    </details>
  );
}
