import { useEffect, useState } from "react";
import { api, errText } from "./api";
import type { CacheStatus } from "./types";

interface Props {
  onClose: () => void;
  onChanged: () => void;
}

export function CacheDialog({ onClose, onChanged }: Props) {
  const [st, setSt] = useState<CacheStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmOn, setConfirmOn] = useState(false);

  useEffect(() => {
    api.cacheStatus().then(setSt).catch((e) => setError(errText(e)));
  }, []);

  const run = async (job: () => Promise<CacheStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setSt(await job());
      onChanged();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
      setConfirmOn(false);
    }
  };

  const enable = () =>
    run(async () => {
      const s = await api.setOfflineEnabled(true);
      // Pierwsza synchronizacja ustala punkt odniesienia; błąd sieci nie przerywa włączenia.
      try { await api.syncNow(); } catch { /* ponowimy w tle */ }
      return api.cacheStatus().catch(() => s);
    });

  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Tryb offline">
      <div className="modal">
        <h3>Tryb offline (pamięć lokalna)</h3>
        {!st ? (
          <p className="hint">Wczytywanie…</p>
        ) : !st.available ? (
          <p className="error" role="alert">Nie udało się otworzyć lokalnej bazy danych (sprawdź, czy działa portfel kluczy systemu). Aplikacja działa tylko online.</p>
        ) : (
          <>
            <p className="hint">
              Gdy ta opcja jest włączona, RustMail zapisuje na tym komputerze kopie wątków, które przeglądasz (temat,
              uczestnicy, treść; bez załączników i bez obrazów z internetu), aby można je było czytać i przeszukiwać bez
              połączenia. Zmiany w Gmailu są dociągane co minutę (tylko to, co się zmieniło).
            </p>
            <p className="hint warn-text">
              Baza jest <strong>szyfrowana</strong> (SQLCipher), a klucz leży w pęku kluczy systemu (portfel KDE), nie obok
              pliku. Chroni to przed skopiowaniem samego pliku z dysku; nie chroni przed programem uruchomionym na Twoim
              koncie, który może poprosić portfel o klucz. Włącz tylko na komputerze, któremu ufasz. Wyłączenie lub
              wyczyszczenie usuwa wszystkie zapisane wiadomości. Wysyłanie i akcje na poczcie nadal wymagają sieci.
            </p>
            <p className="hint">
              Stan: {st.enabled ? (st.ready ? "włączony" : "włączony (trwa przygotowanie)") : "wyłączony"} · zapisanych
              wątków: {st.threads}
            </p>
            <p className="hint">Plik: <code>{st.dbPath}</code></p>
          </>
        )}
        {error && <p className="error" role="alert">{error}</p>}
        <div className="modal-actions">
          {st?.available && !st.enabled && (
            confirmOn ? (
              <button className="primary" disabled={busy} onClick={() => void enable()}>
                {busy ? "Włączanie…" : "Rozumiem, włącz"}
              </button>
            ) : (
              <button className="primary" disabled={busy} onClick={() => setConfirmOn(true)}>Włącz tryb offline</button>
            )
          )}
          {st?.available && st.enabled && (
            <>
              <button disabled={busy} onClick={() => void run(() => api.clearCache())}>Wyczyść zapisane wątki</button>
              <button disabled={busy} onClick={() => void run(() => api.setOfflineEnabled(false))}>Wyłącz i usuń dane</button>
            </>
          )}
          <button disabled={busy} onClick={onClose}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
