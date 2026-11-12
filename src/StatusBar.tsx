import type { AppInfo } from "./types";
import type { Updater } from "./useUpdater";

interface Props {
  info: AppInfo | null;
  u: Updater;
  outboxCount: number;
  onOutbox: () => void;
  onCalendar: () => void;
  /** Pogoda w belce (null = wyłączona). */
  weather: { icon: string; temp: string; title: string } | null;
  onWeather: () => void;
  onSettings: () => void;
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

function CalendarIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true" {...stroke}>
      <rect x="3" y="4" width="18" height="18" rx="2" />
      <path d="M16 2v4M8 2v4M3 10h18" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" aria-hidden="true" {...stroke}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

/** Dolna belka: nazwa i wersja, ikona sprawdzania aktualizacji, a po prawej koło zębate (ustawienia). */
export function StatusBar({ info, u, outboxCount, onOutbox, onCalendar, weather, onWeather, onSettings }: Props) {
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
          <button className="small solid" onClick={() => void u.install()}>Zaktualizuj</button>
          <button className="small" onClick={u.dismiss}>Później</button>
        </>
      )}
      {(phase === "ready" || phase === "rolledback") && (
        <>
          <span>{phase === "ready" ? "Aktualizacja gotowa." : "Przywrócono poprzednią wersję."} Uruchom ponownie, aby ją zastosować.</span>
          <button className="small solid" onClick={u.restart}>Uruchom ponownie</button>
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
      {weather && <button className="linklike plain wx-chip" title={weather.title} aria-label={"Pogoda: " + weather.title} onClick={onWeather}>{weather.icon} {weather.temp}</button>}
      <button className="icon-btn gear" title="Kalendarz" aria-label="Kalendarz" onClick={onCalendar}>
        <CalendarIcon />
      </button>
      <button className="icon-btn gear" title="Ustawienia" aria-label="Ustawienia" onClick={onSettings}>
        <GearIcon />
      </button>
    </footer>
  );
}
