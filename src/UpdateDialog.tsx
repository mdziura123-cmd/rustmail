import { useEffect, useRef } from "react";
import type { Stage, Updater } from "./useUpdater";

const STEPS: { id: Stage; label: string }[] = [
  { id: "wait", label: "Kończenie bieżących operacji" },
  { id: "prepare", label: "Sprawdzanie podpisu wydania" },
  { id: "download", label: "Pobieranie nowej wersji" },
  { id: "checksum", label: "Sprawdzanie sumy kontrolnej" },
  { id: "install", label: "Instalowanie" },
];

const stroke = { fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round", strokeLinejoin: "round" } as const;

function Badge({ kind }: { kind: "update" | "ok" | "err" }) {
  return (
    <div className={"upd-badge " + kind} aria-hidden="true">
      <svg viewBox="0 0 24 24" {...stroke}>
        {kind === "update" && <><path d="M12 4v11" /><path d="M7 10l5 5 5-5" /><path d="M5 20h14" /></>}
        {kind === "ok" && <path d="M5 12.5l4.5 4.5L19 7.5" />}
        {kind === "err" && <><path d="M12 7v6" /><path d="M12 17h.01" /></>}
      </svg>
    </div>
  );
}

function Notes({ text }: { text: string }) {
  const lines = text.split("\n").map((l) => l.replace(/^\s*[-*•]\s*/, "").trim()).filter(Boolean).slice(0, 12);
  if (lines.length === 0) return null;
  return (
    <div className="upd-notes">
      <div className="upd-notes-title">Co nowego</div>
      <ul>{lines.map((l, i) => <li key={i}>{l}</li>)}</ul>
    </div>
  );
}

/** Okno aktualizacji. Od kliknięcia „Zaktualizuj teraz” do zakończenia zasłania całą aplikację,
 *  żeby w trakcie podmiany plików nic się nie zmieniało (klasa modal-back wyłącza też skróty klawiszowe). */
export function UpdateDialog({ u }: { u: Updater }) {
  const box = useRef<HTMLDivElement>(null);
  const showAsk = u.asking && u.phase === "available";
  const showRun = u.phase === "installing";
  const showReady = u.phase === "ready" && !u.readyHidden;
  const showErr = u.phase === "error" && u.installFailed;
  const open = showAsk || showRun || showReady || showErr;
  useEffect(() => { if (open) box.current?.focus(); }, [open, u.phase]);
  if (!open || !u.info) return null;

  const from = u.info.current;
  const to = u.info.latest;
  const cur = STEPS.findIndex((s) => s.id === u.stage);
  const closable = showAsk || showErr;

  return (
    <div className="modal-back upd-back" role="dialog" aria-modal="true" aria-labelledby="upd-title"
      onKeyDown={(e) => { if (e.key === "Escape" && closable) { if (showAsk) u.cancelAsk(); else u.dismiss(); } }}>
      <div className="modal upd-box" ref={box} tabIndex={-1}>
        {showAsk && (
          <>
            <Badge kind="update" />
            <h3 id="upd-title">Dostępna nowa wersja RustMail</h3>
            <div className="upd-ver"><span>{from}</span><span aria-hidden="true">→</span><b>{to}</b></div>
            <Notes text={u.info.notes} />
            <p className="hint">
              Na czas aktualizacji (zwykle minuta–dwie) aplikacja będzie zablokowana: sprawdzanie poczty i wysyłka
              kolejki poczekają, a otwarte szkice są zapisywane automatycznie. Nic nie zostanie utracone.
            </p>
            <div className="modal-actions">
              <button className="primary" onClick={() => void u.install()}>Zaktualizuj teraz</button>
              <button onClick={u.cancelAsk}>Później</button>
            </div>
          </>
        )}

        {showRun && (
          <>
            <Badge kind="update" />
            <h3 id="upd-title">Aktualizacja do wersji {to}</h3>
            <p className="hint">Nie zamykaj aplikacji. Po zakończeniu poprosimy o ponowne uruchomienie.</p>
            <ol className="upd-steps" aria-live="polite">
              {STEPS.map((s, i) => {
                const st = i < cur ? "done" : i === cur ? "now" : "todo";
                return (
                  <li key={s.id} className={st} aria-current={st === "now" ? "step" : undefined}>
                    <span className="upd-dot" aria-hidden="true">{st === "done" ? "✓" : ""}</span>
                    <span className="upd-label">{s.label}</span>
                    {s.id === "download" && st === "now" && <span className="upd-pct">{u.progress === null ? "" : `${u.progress}%`}</span>}
                  </li>
                );
              })}
            </ol>
            <div className="progress upd-bar" role="progressbar" aria-label="Postęp aktualizacji" aria-valuemin={0} aria-valuemax={100}
              aria-valuenow={u.stage === "download" && u.progress !== null ? u.progress : undefined}>
              <div className={"progress-fill" + (u.stage === "download" && u.progress !== null ? "" : " indeterminate")}
                style={u.stage === "download" && u.progress !== null ? { width: `${u.progress}%` } : undefined} />
            </div>
          </>
        )}

        {showReady && (
          <>
            <Badge kind="ok" />
            <h3 id="upd-title">Wersja {to} jest zainstalowana</h3>
            <p className="hint">
              Podpis i suma kontrolna się zgadzają. Uruchom RustMail ponownie, żeby przejść na nową wersję.
              Poprzednia wersja jest zachowana – w razie problemów przywrócisz ją w Ustawieniach.
            </p>
            <div className="modal-actions">
              <button className="primary" disabled={u.restarting} onClick={() => void u.restart()}>
                {u.restarting ? "Uruchamianie…" : "Uruchom ponownie teraz"}
              </button>
              <button disabled={u.restarting} onClick={u.later}>Później</button>
            </div>
          </>
        )}

        {showErr && (
          <>
            <Badge kind="err" />
            <h3 id="upd-title">Aktualizacja się nie udała</h3>
            <p className="error" role="alert">{u.error}</p>
            <p className="hint">Obecna wersja ({from}) działa dalej bez zmian – możesz spróbować ponownie później.</p>
            <div className="modal-actions">
              <button className="primary" onClick={() => void u.install()}>Spróbuj ponownie</button>
              <button onClick={u.dismiss}>Zamknij</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
