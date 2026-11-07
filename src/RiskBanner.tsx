import type { MessageView, Risk } from "./types";

export function worstRisk(messages: MessageView[]): { risk: Risk; from: string } | null {
  let best: { risk: Risk; from: string } | null = null;
  for (const m of messages) {
    if (m.risk.level === "none") continue;
    if (!best || m.risk.score > best.risk.score) best = { risk: m.risk, from: m.from };
  }
  return best;
}

interface Props {
  messages: MessageView[];
  dismissed: boolean;
  onSafe: (safe: boolean) => void;
}

/** Pasek ostrzeżenia nad konwersacją: powody oceny po polsku i przycisk „fałszywy alarm”. */
export function RiskBanner({ messages, dismissed, onSafe }: Props) {
  const worst = worstRisk(messages);
  if (!worst) return null;
  if (dismissed) {
    return (
      <div className="risk-note" role="status">
        Ta konwersacja została przez Ciebie oznaczona jako bezpieczna.{" "}
        <button className="linklike plain" onClick={() => onSafe(false)}>Cofnij</button>
      </div>
    );
  }
  const high = worst.risk.level === "high";
  return (
    <div className={"risk-banner " + (high ? "high" : "warn")} role="alert">
      <strong>
        {high
          ? "Uwaga: ta wiadomość wygląda na próbę oszustwa (phishing)."
          : "Ta wiadomość ma podejrzane cechy – zachowaj ostrożność."}
      </strong>
      <ul>
        {worst.risk.reasons.map((r) => <li key={r}>{r}</li>)}
      </ul>
      <p className="hint">
        Ocena jest wyliczana na Twoim komputerze według reguł i może się mylić. Nie klikaj linków, nie podawaj haseł
        ani kodów i nie otwieraj załączników, jeśli nie masz pewności, kto jest nadawcą.
      </p>
      <button className="small" onClick={() => onSafe(true)}>To fałszywy alarm – oznacz jako bezpieczne</button>
    </div>
  );
}
