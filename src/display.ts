// Wygląd: skala interfejsu (powiększenie całego widoku) i podpisy pod ikonami (przydatne na dotyku).
import { getCurrentWebview } from "@tauri-apps/api/webview";

const KEY_UNDO = "rm.undo";
export const UNDO_DELAYS = [0, 5, 10, 20, 30] as const;
export function getUndo(): number {
  const n = Number(read(KEY_UNDO));
  return (UNDO_DELAYS as readonly number[]).includes(n) ? n : 5;
}
export function setUndo(n: number) {
  write(KEY_UNDO, String((UNDO_DELAYS as readonly number[]).includes(n) ? n : 5));
}

export const SCALES = [0.9, 1, 1.1, 1.25, 1.5] as const;
const KEY_SCALE = "rm.scale";
const KEY_LABELS = "rm.labels";

function read(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* pamięć niedostępna – ustawienie działa do zamknięcia */ }
}

export function getScale(): number {
  const n = Number(read(KEY_SCALE));
  return (SCALES as readonly number[]).includes(n) ? n : 1;
}
export function getLabels(): boolean {
  return read(KEY_LABELS) === "1";
}

export function setScale(n: number) {
  const v = (SCALES as readonly number[]).includes(n) ? n : 1;
  write(KEY_SCALE, String(v));
  void getCurrentWebview().setZoom(v).catch(() => undefined);
}
export function setLabels(on: boolean) {
  write(KEY_LABELS, on ? "1" : "0");
  document.documentElement.classList.toggle("show-labels", on);
}

/** Stosuje zapisane ustawienia przy starcie. */
export function applyDisplay() {
  document.documentElement.classList.toggle("show-labels", getLabels());
  const s = getScale();
  if (s !== 1) void getCurrentWebview().setZoom(s).catch(() => undefined);
}
