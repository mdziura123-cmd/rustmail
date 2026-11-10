// Dźwięk nowej wiadomości: syntezowany w locie (bez plików dźwiękowych). Trzy brzmienia do wyboru.
export type SoundId = "off" | "1" | "2" | "3";
export const SOUNDS: { id: SoundId; label: string }[] = [
  { id: "off", label: "Wyłączony" },
  { id: "1", label: "Dzwonek" },
  { id: "2", label: "Kropla" },
  { id: "3", label: "Akord" },
];

const KEY = "rm.sound";

export function loadSound(): SoundId {
  try {
    const v = localStorage.getItem(KEY);
    return v === "1" || v === "2" || v === "3" ? v : "off";
  } catch { return "off"; }
}
export function saveSound(id: SoundId) {
  try { localStorage.setItem(KEY, id); } catch { /* pamięć niedostępna */ }
}

// Nuty: [częstotliwość Hz, początek s, długość s, głośność]
const PATTERNS: Record<"1" | "2" | "3", { type: OscillatorType; notes: [number, number, number, number][] }> = {
  "1": { type: "sine", notes: [[880, 0, 0.5, 0.22], [1318.5, 0.14, 0.7, 0.18]] },
  "2": { type: "sine", notes: [[1046.5, 0, 0.16, 0.25], [523.25, 0.1, 0.35, 0.2]] },
  "3": { type: "triangle", notes: [[523.25, 0, 0.6, 0.14], [659.25, 0.05, 0.6, 0.14], [783.99, 0.1, 0.7, 0.14]] },
};

let ctx: AudioContext | null = null;

/** Odtwarza wybrany dźwięk. Błędy (brak urządzenia audio) są po cichu pomijane. */
export function playSound(id: SoundId) {
  if (id === "off") return;
  try {
    const AC = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AC) return;
    ctx = ctx ?? new AC();
    if (ctx.state === "suspended") void ctx.resume();
    const t0 = ctx.currentTime + 0.02;
    const pat = PATTERNS[id];
    for (const [freq, start, dur, vol] of pat.notes) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = pat.type;
      osc.frequency.value = freq;
      gain.gain.setValueAtTime(0.0001, t0 + start);
      gain.gain.exponentialRampToValueAtTime(vol, t0 + start + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, t0 + start + dur);
      osc.connect(gain).connect(ctx.destination);
      osc.start(t0 + start);
      osc.stop(t0 + start + dur + 0.05);
    }
  } catch { /* brak dźwięku */ }
}
