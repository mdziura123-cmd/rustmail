import type { Updater } from "./useUpdater";

export function UpdateBanner({ u }: { u: Updater }) {
  if (!u.visible) return null;
  const { phase, info } = u;
  return (
    <div className="banner" role="status">
      {phase === "checking" && <span>Sprawdzanie aktualizacji…</span>}
      {phase === "uptodate" && (
        <>
          <span>Masz najnowszą wersję{info ? ` (${info.current})` : ""}.</span>
          <button onClick={u.dismiss}>Zamknij</button>
        </>
      )}
      {phase === "available" && info && (
        <>
          <span>Dostępna nowa wersja <b>{info.latest}</b> (masz {info.current}). Zaktualizować?</span>
          {info.notes && (
            <details>
              <summary>Co nowego</summary>
              <pre>{info.notes}</pre>
            </details>
          )}
          <button className="solid" onClick={() => void u.install()}>Zaktualizuj teraz</button>
          <button onClick={u.dismiss}>Później</button>
        </>
      )}
      {phase === "installing" && <span>Pobieranie i weryfikacja aktualizacji (SHA-256)… Nie zamykaj aplikacji.</span>}
      {phase === "ready" && (
        <>
          <span>Aktualizacja gotowa. Uruchom ponownie, aby ją zastosować.</span>
          <button className="solid" onClick={u.restart}>Uruchom ponownie teraz</button>
          <button onClick={u.dismiss}>Później</button>
        </>
      )}
      {phase === "rolledback" && (
        <>
          <span>Przywrócono poprzednią wersję. Uruchom ponownie, aby z niej skorzystać.</span>
          <button className="solid" onClick={u.restart}>Uruchom ponownie teraz</button>
          <button onClick={u.dismiss}>Później</button>
        </>
      )}
      {phase === "error" && (
        <>
          <span className="error">{u.error}</span>
          <button onClick={u.dismiss}>Zamknij</button>
        </>
      )}
    </div>
  );
}
