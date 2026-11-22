import { useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { appLock } from "./appLock";

/** Ekran blokady zasłaniający całą aplikację. Klasa modal-back wyłącza skróty klawiszowe pod spodem. */
export function LockScreen() {
  const [pin, setPin] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); }, []);

  const submit = async () => {
    if (!pin || busy) return;
    setBusy(true); setErr(null);
    try {
      if (await api.lockVerify(pin)) {
        setPin("");
        appLock.unlock();
      } else {
        setErr("Nieprawidłowy kod.");
        setPin("");
      }
    } catch (e) {
      setErr(errText(e));
    } finally {
      setBusy(false);
      input.current?.focus();
    }
  };

  return (
    <div className="modal-back lock-back" role="dialog" aria-modal="true" aria-label="Aplikacja zablokowana">
      <form className="modal lock-box" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
        <svg className="lock-icon" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 11h12v10H6zM8 11V7a4 4 0 0 1 8 0v4M12 15v2" />
        </svg>
        <h3>RustMail jest zablokowany</h3>
        <p className="hint">Wpisz kod, aby wrócić do poczty. Powiadomienia w tym czasie nie pokazują nadawcy ani tematu.</p>
        <input ref={input} type="password" autoComplete="off" spellCheck={false}
          aria-label="Kod blokady" placeholder="Kod" value={pin} disabled={busy}
          onChange={(e) => setPin(e.target.value)} />
        {err && <p className="error" role="alert">{err}</p>}
        <div className="modal-actions">
          <button type="submit" className="primary" disabled={!pin || busy}>{busy ? "Sprawdzanie…" : "Odblokuj"}</button>
        </div>
      </form>
    </div>
  );
}

/** Ustawienia blokady (zakładka „Bezpieczeństwo”). */
export function LockSettings({ idle, onIdle }: { idle: number; onIdle: (n: number) => void }) {
  const [has, setHas] = useState<boolean | null>(null);
  const [cur, setCur] = useState("");
  const [pin, setPin] = useState("");
  const [pin2, setPin2] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const refresh = () => { void api.lockStatus().then((h) => { setHas(h); appLock.setEnabled(h); }).catch((e) => setErr(errText(e))); };
  useEffect(refresh, []);

  const save = async () => {
    setErr(null); setNote(null);
    if (pin !== pin2) { setErr("Kody się różnią."); return; }
    try {
      await api.lockSet(has ? cur : null, pin);
      setCur(""); setPin(""); setPin2("");
      setNote(has ? "Kod zmieniony." : "Blokada włączona.");
      refresh();
    } catch (e) { setErr(errText(e)); }
  };
  const remove = async () => {
    setErr(null); setNote(null);
    try {
      await api.lockClear(cur);
      setCur("");
      setNote("Blokada wyłączona.");
      refresh();
    } catch (e) { setErr(errText(e)); }
  };

  if (has === null) return <p className="hint">Wczytywanie…</p>;
  return (
    <>
      <h4>Blokada kodem</h4>
      <p className="hint">
        Po ustawieniu kodu RustMail blokuje się przy uruchomieniu i po wybranym czasie bezczynności. Kod (PIN albo hasło,
        co najmniej 4 znaki) nie jest nigdzie zapisany wprost – w systemowym magazynie haseł leży tylko jego skrót.
        Blokada chroni przed podejrzeniem poczty przy włączonym komputerze; nie zastępuje blokady ekranu systemu.
      </p>
      <div className="lock-form">
        {has && (
          <input type="password" autoComplete="off" aria-label="Obecny kod" placeholder="Obecny kod" value={cur}
            onChange={(e) => setCur(e.target.value)} />
        )}
        <input type="password" autoComplete="off" aria-label={has ? "Nowy kod" : "Kod"} placeholder={has ? "Nowy kod" : "Kod"} value={pin}
          onChange={(e) => setPin(e.target.value)} />
        <input type="password" autoComplete="off" aria-label="Powtórz kod" placeholder="Powtórz kod" value={pin2}
          onChange={(e) => setPin2(e.target.value)} />
      </div>
      <div className="modal-actions wrap">
        <button disabled={pin.length < 4 || !pin2 || (has === true && !cur)} onClick={() => void save()}>{has ? "Zmień kod" : "Włącz blokadę"}</button>
        {has && <button disabled={!cur} onClick={() => void remove()}>Wyłącz blokadę</button>}
        {has && <button onClick={() => appLock.lock()}>Zablokuj teraz</button>}
      </div>
      {has && (
        <div className="check sound-row">
          <label htmlFor="lock-idle">Blokuj po bezczynności</label>
          <select id="lock-idle" value={idle} onChange={(e) => onIdle(Number(e.target.value))}>
            <option value={0}>nigdy (tylko przy uruchomieniu i ręcznie)</option>
            {[1, 5, 10, 15, 30, 60].map((n) => <option key={n} value={n}>{n} min</option>)}
          </select>
        </div>
      )}
      {err && <p className="error" role="alert">{err}</p>}
      {note && <p className="hint" role="status">{note}</p>}
    </>
  );
}
