/** Przycisk „Odbierz wiadomości”: koperta, która w czasie pobierania jest w ruchu. */
export function ReceiveButton({ busy, onClick }: { busy: boolean; onClick: () => void }) {
  return (
    <button className={"icon-btn receive" + (busy ? " busy" : "")} title="Odbierz wiadomości" aria-label="Odbierz wiadomości"
      aria-busy={busy} disabled={busy} onClick={onClick}>
      <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2"
        strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="5" width="18" height="14" rx="3" />
        <path d="M4 8l8 6 8-6" />
      </svg>
    </button>
  );
}
