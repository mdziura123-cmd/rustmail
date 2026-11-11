import type { OutboxItem, SchedItem, SnoozeItem } from "./types";
import { cleanDisplay, fmtFull } from "./util";

interface Props {
  items: OutboxItem[];
  scheduled: SchedItem[];
  snoozed: SnoozeItem[];
  onCancelScheduled: (id: number) => void | Promise<void>;
  onWake: (threadId: string) => void;
  onSendNow: () => void;
  onRetry: (id: number) => void | Promise<void>;
  onDelete: (id: number) => void | Promise<void>;
  onClose: () => void;
}

const STATE: Record<string, string> = {
  queued: "czeka na sieć",
  sending: "wysyłanie…",
  failed: "nie udało się wysłać",
  uncertain: "nie wiadomo, czy wyszła",
};

export function OutboxDialog({ items, scheduled, snoozed, onCancelScheduled, onWake, onSendNow, onRetry, onDelete, onClose }: Props) {
  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Do wysłania">
      <div className="modal">
        <h3>Do wysłania</h3>
        {items.length === 0 ? (
          <p className="hint">Kolejka wysyłania jest pusta.</p>
        ) : (
          <ul className="outbox-list">
            {items.map((i) => (
              <li key={i.id}>
                <strong>{cleanDisplay(i.subject, 80) || "(bez tematu)"}</strong>
                <span className="hint"> do: {cleanDisplay(i.to, 120)} · {fmtFull(i.createdMs)}</span>
                <span className={"outbox-state " + i.state}>{STATE[i.state] ?? i.state}</span>
                {i.error && <span className="error">{cleanDisplay(i.error, 200)}</span>}
                {i.state === "uncertain" && (
                  <span className="hint">
                    Aplikacja zamknęła się w trakcie wysyłania. Sprawdź folder „Wysłane”: jeśli wiadomości tam nie ma, wyślij ją ponownie.
                  </span>
                )}
                <span className="outbox-actions">
                  {(i.state === "failed" || i.state === "uncertain") && (
                    <button className="small" onClick={() => void onRetry(i.id)}>Wyślij ponownie</button>
                  )}
                  {i.state !== "sending" && <button className="small" onClick={() => void onDelete(i.id)}>Usuń</button>}
                </span>
              </li>
            ))}
          </ul>
        )}
        {scheduled.length > 0 && (
          <>
            <h4>Zaplanowane do wysłania</h4>
            <ul className="outbox-list">
              {scheduled.map((s) => (
                <li key={s.id}>
                  <strong>{cleanDisplay(s.subject, 80) || "(bez tematu)"}</strong>
                  <span className="hint"> do: {cleanDisplay(s.to, 120)} · wyśle się {fmtFull(s.sendAtMs)}</span>
                  <span className="outbox-actions"><button className="small" onClick={() => void onCancelScheduled(s.id)}>Anuluj wysyłkę</button></span>
                </li>
              ))}
            </ul>
            <p className="hint">Zaplanowane wiadomości wysyłają się, gdy aplikacja działa (także w tle). Jeśli była wyłączona o tej porze, wyślą się po jej uruchomieniu.</p>
          </>
        )}
        {snoozed.length > 0 && (
          <>
            <h4>Odłożone wiadomości</h4>
            <ul className="outbox-list">
              {snoozed.map((s) => (
                <li key={s.threadId}>
                  <strong>{cleanDisplay(s.subject, 80) || "(bez tematu)"}</strong>
                  <span className="hint"> wróci {fmtFull(s.wakeMs)}</span>
                  <span className="outbox-actions"><button className="small" onClick={() => onWake(s.threadId)}>Przywróć teraz</button></span>
                </li>
              ))}
            </ul>
          </>
        )}
        <p className="hint">
          Wiadomości w kolejce leżą na dysku w szyfrowanej bazie lokalnej do czasu wysłania. Wysyłają się automatycznie co minutę,
          gdy aplikacja jest uruchomiona i jest połączenie. Wiadomość z błędem nie jest wysyłana ponownie sama.
        </p>
        <div className="modal-actions">
          <button onClick={onSendNow}>Wyślij teraz</button>
          <button className="primary" onClick={onClose}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
