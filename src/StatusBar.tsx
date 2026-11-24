import type { AppInfo } from "./types";
import type { Updater } from "./useUpdater";

interface Props {
  info: AppInfo | null;
  u: Updater;
  outboxCount: number;
  onOutbox: () => void;
}

const stroke = { fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round", strokeLinejoin: "round" } as const;

function RefreshIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true" {...stroke}>
      <path d="M21 12a9 9 0 1 1-2.64-6.36" />
      <path d="M21 3v6h-6" />
    </svg>
  );
}

/** Dolna belka: nazwa i wersja, sprawdzanie aktualizacji, kolejka do wysłania. Ustawienia i kalendarz są na bocznej belce. */
export function StatusBar({ info, u, outboxCount, onOutbox }: Props) {
  const { phase } = u;

  // Podczas aktualizacji na belce jest tylko pasek postępu.
  if (phase === "installing") {
    const p = u.progress;
    return (
      <footer className="status updating" role="status">
        <div className="progress" role="progressbar" aria-label="Postęp aktualizacji" aria-valuemin={0} aria-valuemax={100}
          aria-valuenow={p ?? undefined}>
          <div className={"progress-fill" + (p === null ? " indeterminate" : "")} style={p === null ? undefined : { width: `${p}%` }} />
        </div>
        <span className="progress-text">{p === null ? "Pobieranie…" : `${p}%`}</span>
      </footer>
    );
  }

  const checking = phase === "checking";
  return (
    <footer className="status">
      <span>{info ? `${info.name} ${info.version}${info.appimage ? " · AppImage" : ""}` : "Tryb podglądu w przeglądarce"}</span>
      <button className={"icon-btn upd" + (checking ? " spin" : "")} title="Sprawdź aktualizację" aria-label="Sprawdź aktualizację"
        disabled={checking} onClick={() => void u.check(true)}>
        <RefreshIcon />
      </button>
      {checking && <span>Sprawdzanie…</span>}
      {phase === "uptodate" && <span>Masz najnowszą wersję{u.info ? ` (${u.info.current})` : ""}.</span>}
      {phase === "available" && u.info && (
        <>
          <span title={u.info.notes || undefined}>Dostępna nowa wersja <b>{u.info.latest}</b>. Zaktualizować?</span>
          <button className="small solid" onClick={u.ask}>Zaktualizuj…</button>
          <button className="small" onClick={u.dismiss}>Później</button>
        </>
      )}
      {(phase === "ready" || phase === "rolledback") && (
        <>
          <span>{phase === "ready" ? "Aktualizacja gotowa." : "Przywrócono poprzednią wersję."} Uruchom ponownie, aby ją zastosować.</span>
          <button className="small solid" onClick={() => void u.restart()}>Uruchom ponownie</button>
          <button className="small" onClick={u.dismiss}>Później</button>
        </>
      )}
      {phase === "error" && (
        <>
          <span className="error">{u.error}</span>
          <button className="small" onClick={u.dismiss}>Zamknij</button>
        </>
      )}
      <span className="grow" />
      {outboxCount > 0 && <button className="linklike plain" onClick={onOutbox}>Do wysłania i odłożone ({outboxCount})</button>}
    </footer>
  );
}
