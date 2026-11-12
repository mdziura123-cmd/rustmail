import { useCallback, useEffect, useState } from "react";
import { api, errText } from "./api";

export interface WeatherPref { on: boolean; name: string; lat: number; lon: number }
export interface WeatherDay { date: string; code: number; max: number; min: number; rain: number | null }
export interface WeatherData {
  temp: number; feels: number; code: number; wind: number; humidity: number; isDay: boolean; days: WeatherDay[];
}
export interface Place { name: string; region: string; country: string; lat: number; lon: number }

const KEY = "rm.weather";
const EVENT = "rm-weather";

export function loadPref(): WeatherPref | null {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (!v || typeof v !== "object") return null;
    const o = v as Record<string, unknown>;
    if (typeof o.name !== "string" || typeof o.lat !== "number" || typeof o.lon !== "number") return null;
    if (o.lat < -90 || o.lat > 90 || o.lon < -180 || o.lon > 180) return null;
    return { on: o.on === true, name: o.name.slice(0, 80), lat: o.lat, lon: o.lon };
  } catch { return null; }
}

export function savePref(p: WeatherPref | null): void {
  try {
    if (p) localStorage.setItem(KEY, JSON.stringify(p)); else localStorage.removeItem(KEY);
  } catch { /* pamięć niedostępna */ }
  window.dispatchEvent(new Event(EVENT));
}

/** Opis i ikona dla kodu pogody WMO (https://open-meteo.com/en/docs). */
export function describe(code: number, isDay = true): { icon: string; text: string } {
  if (code === 0) return isDay ? { icon: "☀️", text: "Bezchmurnie" } : { icon: "🌙", text: "Bezchmurna noc" };
  if (code === 1) return { icon: isDay ? "🌤️" : "🌙", text: "Przeważnie bezchmurnie" };
  if (code === 2) return { icon: "⛅", text: "Częściowe zachmurzenie" };
  if (code === 3) return { icon: "☁️", text: "Pochmurno" };
  if (code === 45 || code === 48) return { icon: "🌫️", text: "Mgła" };
  if (code >= 51 && code <= 55) return { icon: "🌦️", text: "Mżawka" };
  if (code === 56 || code === 57) return { icon: "🌧️", text: "Marznąca mżawka" };
  if (code === 61 || code === 63 || code === 65) return { icon: "🌧️", text: code === 65 ? "Silny deszcz" : "Deszcz" };
  if (code === 66 || code === 67) return { icon: "🌧️", text: "Marznący deszcz" };
  if (code >= 71 && code <= 75) return { icon: "🌨️", text: "Śnieg" };
  if (code === 77) return { icon: "🌨️", text: "Ziarna śniegu" };
  if (code >= 80 && code <= 82) return { icon: "🌦️", text: "Przelotny deszcz" };
  if (code === 85 || code === 86) return { icon: "🌨️", text: "Przelotny śnieg" };
  if (code === 95) return { icon: "⛈️", text: "Burza" };
  if (code === 96 || code === 99) return { icon: "⛈️", text: "Burza z gradem" };
  return { icon: "❓", text: "Brak opisu" };
}

export const fmtTemp = (t: number) => `${Math.round(t)}°`;

/** Pogoda dla miejscowości wybranej przez użytkownika; bez wybranej miejscowości i zgody nic nie pobiera. */
export function useWeather() {
  const [pref, setPref] = useState<WeatherPref | null>(loadPref);
  const [data, setData] = useState<WeatherData | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const h = () => setPref(loadPref());
    window.addEventListener(EVENT, h);
    return () => window.removeEventListener(EVENT, h);
  }, []);

  const active = pref && pref.on ? pref : null;
  const lat = active?.lat;
  const lon = active?.lon;

  const refresh = useCallback(async () => {
    if (lat === undefined || lon === undefined) return;
    setLoading(true);
    try {
      setData(await api.weatherGet(lat, lon));
      setErr(null);
    } catch (e) { setErr(errText(e)); } finally { setLoading(false); }
  }, [lat, lon]);

  useEffect(() => {
    if (lat === undefined || lon === undefined) { setData(null); setErr(null); return; }
    void refresh();
    const t = window.setInterval(() => void refresh(), 30 * 60_000);
    return () => window.clearInterval(t);
  }, [lat, lon, refresh]);

  return { pref: active, data, err, loading, refresh };
}
