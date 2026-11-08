import type { Account } from "./types";

interface Props {
  accounts: Account[];
  current: string;
  busy: boolean;
  onSwitch: (email: string) => void;
  onAdd: () => void;
  onSignOut: () => void;
}

/** Przełącznik kont w panelu bocznym: bieżący adres, lista kont, dodawanie i wylogowanie bieżącego konta. */
export function AccountSwitcher({ accounts, current, busy, onSwitch, onAdd, onSignOut }: Props) {
  const close = (e: React.MouseEvent<HTMLElement>) => {
    const d = e.currentTarget.closest("details");
    if (d) d.removeAttribute("open");
  };
  return (
    <details className="accounts">
      <summary title="Konta">
        <span className="acc-email">{current}</span>
        {accounts.length > 1 && <span className="acc-count">{accounts.length}</span>}
      </summary>
      <div className="acc-pop" role="menu">
        {accounts.map((a) => (
          <button key={a.email} role="menuitemradio" aria-checked={a.active} disabled={busy || a.active}
            className={a.active ? "acc-item active" : "acc-item"}
            onClick={(e) => { close(e); onSwitch(a.email); }}>
            {a.active ? "✓ " : ""}{a.email}
          </button>
        ))}
        <button role="menuitem" disabled={busy} className="acc-item" onClick={(e) => { close(e); onAdd(); }}>
          + Dodaj konto Gmail
        </button>
        <button role="menuitem" disabled={busy} className="acc-item danger" onClick={(e) => { close(e); onSignOut(); }}>
          Wyloguj to konto
        </button>
      </div>
    </details>
  );
}
