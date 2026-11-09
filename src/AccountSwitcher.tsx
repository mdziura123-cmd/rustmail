import type { Account, ImapAccount } from "./types";

interface Props {
  gmail: Account[];
  imap: ImapAccount[];
  /** "gmail" | "all" | "imap:adres" */
  view: string;
  label: string;
  busy: boolean;
  onGmail: (email: string) => void;
  onImap: (email: string) => void;
  onAll: () => void;
  onAddGmail: () => void;
  onAddImap: () => void;
  onSignOut: () => void;
  onSignOutImap: (email: string) => void;
}

/** Przełącznik kont: zalogowane konta, widok „wszystkie razem” i osobna sekcja dodawania kont. */
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
        {total > 1 && (
          <>
            <div className="acc-title">Widok</div>
            {item(p.view === "all", "Wszystkie skrzynki razem (Odebrane)", p.onAll, "all")}
          </>
        )}
        <div className="acc-title">Dodaj konto</div>
        <button role="menuitem" disabled={p.busy} className="acc-item" onClick={(e) => { close(e); p.onAddGmail(); }}>+ Gmail</button>
        <button role="menuitem" className="acc-item" onClick={(e) => { close(e); p.onAddImap(); }}>+ WP / inne konto IMAP</button>
        <div className="acc-title">Wylogowanie</div>
        {p.view.startsWith("imap:") ? (
          <button role="menuitem" className="acc-item danger" onClick={(e) => { close(e); p.onSignOutImap(p.view.slice(5)); }}>Wyloguj konto {p.view.slice(5)}</button>
        ) : (
          <button role="menuitem" disabled={p.busy} className="acc-item danger" onClick={(e) => { close(e); p.onSignOut(); }}>Wyloguj konto Gmail</button>
        )}
      </div>
    </details>
  );
}
