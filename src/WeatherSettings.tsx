import { useState } from "react";
import { api, errText } from "./api";
import { loadPref, savePref, type Place } from "./weather";

export function WeatherSettings() {
  const [pref, setPref] = useState(loadPref);
  const [q, setQ] = useState("");
  const [found, setFound] = useState<Place[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const search = async () => {
    setErr(null); setBusy(true); setFound(null);
    try {
      const r = await api.weatherSearch(q);
      setFound(r);
      if (r.length === 0) setErr("Nie znaleziono takiej miejscowości.");
    } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const pick = (p: Place) => {
    const next = { on: true, name: p.region ? `${p.name} (${p.region})` : p.name, lat: p.lat, lon: p.lon };
    savePref(next); setPref(next); setFound(null); setQ("");
  };
  const toggle = (on: boolean) => {
    if (!pref) return;
    const next = { ...pref, on };
    savePref(next); setPref(next);
  };
  const clear = () => { savePref(null); setPref(null); };

  return (
    <div className="rule-box">
      <h4>Pogoda</h4>
      <p className="hint">
        Domyślnie wyłączona. Miejscowość wybierasz sam: program nie sprawdza Twojego położenia. Po włączeniu co 30 minut wysyła do serwisu
        Open-Meteo.com (bez konta i klucza) tylko współrzędne wybranej miejscowości (serwis zobaczy też Twój adres IP). Wybór jest zapisany
        tylko na tym komputerze, nie trafia do kopii zapasowej ani synchronizacji i nie ma do niego dostępu poczta.
      </p>
      {pref && (
        <>
          <label className="check">
            <input type="checkbox" checked={pref.on} onChange={(e) => toggle(e.target.checked)} />
            Pokazuj pogodę dla: {pref.name}
          </label>
          <div className="modal-actions"><button className="small" onClick={clear}>Usuń miejscowość i wyłącz pogodę</button></div>
        </>
      )}
      <div className="modal-actions wrap">
        <input value={q} maxLength={80} placeholder={pref ? "Zmień miejscowość…" : "Nazwa miejscowości, np. Kraków"} aria-label="Miejscowość"
          onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") void search(); }} />
        <button disabled={busy || q.trim().length < 2} onClick={() => void search()}>{busy ? "Szukanie…" : "Szukaj"}</button>
      </div>
      {found && found.length > 0 && (
        <ul className="addr-list">
          {found.map((p) => (
            <li key={`${p.lat},${p.lon}`}>
              <span>{p.name}{p.region ? `, ${p.region}` : ""}{p.country ? `, ${p.country}` : ""}</span>
              <button className="small" onClick={() => pick(p)}>Wybierz</button>
            </li>
          ))}
        </ul>
      )}
      {err && <p className="error" role="alert">{err}</p>}
    </div>
  );
}
