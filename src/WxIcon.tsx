import { describe } from "./weather";

/** Rodzaj animacji ikony pogody (CSS: .wx-sun, .wx-cloud, .wx-rain …). */
export function wxKind(code: number, isDay: boolean): string {
  if (code === 0 || code === 1) return isDay ? "sun" : "moon";
  if (code === 2 || code === 3) return "cloud";
  if (code === 45 || code === 48) return "fog";
  if ((code >= 51 && code <= 67) || (code >= 80 && code <= 82)) return "rain";
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return "snow";
  if (code >= 95) return "storm";
  return "cloud";
}

/** Ikona pogody z delikatną animacją (wyłączaną przy ustawieniu systemu „ogranicz ruch”). */
export function WxIcon({ code, isDay, small }: { code: number; isDay: boolean; small?: boolean }) {
  return <span className={`wx-ico wx-${wxKind(code, isDay)}${small ? " wx-small" : ""}`} aria-hidden="true">{describe(code, isDay).icon}</span>;
}
