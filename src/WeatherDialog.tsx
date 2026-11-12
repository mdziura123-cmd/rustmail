import { useEffect } from "react";
import { describe, fmtTemp, type WeatherData, type WeatherPref } from "./weather";

interface Props {
  pref: WeatherPref;
  data: WeatherData | null;
  err: string | null;
  loading: boolean;
  onRefresh: () => void;
  onClose: () => void;
}

function dayName(date: string, i: number): string {
  if (i === 0) return "Dziś";
  if (i === 1) return "Jutro";
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("pl-PL", { weekday: "long", day: "numeric", month: "numeric" });
}

export function WeatherDialog({ pref, data, err, loading, onRefresh, onClose }: Props) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);
  const cur = data ? describe(data.code, data.isDay) : null;
  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Pogoda">
      <div className="modal weather-modal">
        <h3>Pogoda: {pref.name}</h3>
        {err && <p className="error" role="alert">{err}</p>}
        {!data && !err && <p className="hint">Pobieranie…</p>}
        {data && cur && (
          <>
            <div className="wx-now">
              <span className="wx-icon" aria-hidden="true">{cur.icon}</span>
              <span className="wx-temp">{fmtTemp(data.temp)}C</span>
              <span>
                <b>{cur.text}</b>
                <br />
                <span className="hint">odczuwalna {fmtTemp(data.feels)}C · wiatr {Math.round(data.wind)} km/h · wilgotność {Math.round(data.humidity)}%</span>
              </span>
            </div>
            <ul className="wx-days">
              {data.days.map((d, i) => {
                const w = describe(d.code);
                return (
                  <li key={d.date}>
                    <span className="wx-dn">{dayName(d.date, i)}</span>
                    <span title={w.text}>{w.icon} {w.text}</span>
                    <span><b>{fmtTemp(d.max)}</b> / {fmtTemp(d.min)}{d.rain !== null ? ` · opady ${Math.round(d.rain)}%` : ""}</span>
                  </li>
                );
              })}
            </ul>
          </>
        )}
        <p className="hint">Dane: Open-Meteo.com. Miejscowość wybierasz Ty (Ustawienia → Poczta → Pogoda); program nie ustala Twojego położenia.</p>
        <div className="modal-actions">
          <button disabled={loading} onClick={onRefresh}>{loading ? "Odświeżanie…" : "Odśwież"}</button>
          <button className="primary" onClick={onClose}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
