import { ActIcon } from "./ActIcon";
import { setFlagsFirst, useFlagsFirst } from "./listPrefs";
import { PRIOS } from "./prioSort";
import { prio, usePrioLevels } from "./prio";

const FLAG = "M5 3v18M5 4h11l-2 4 2 4H5";

/** Mała kolorowa flaga przy wiadomości na liście. */
export function PrioFlag({ level }: { level: number }) {
  const p = PRIOS.find((x) => x.level === level);
  if (!p) return null;
  return (
    <svg className="prio-flag" viewBox="0 0 24 24" width="14" height="14" role="img" aria-label={p.label}>
      <title>{p.label}</title>
      <path d={FLAG} fill={p.color} stroke={p.color} strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" />
    </svg>
  );
}

/** Menu „Flaga” w pasku akcji czytnika: wybór ważności albo usunięcie flagi. */
export function PrioMenu({ itemKey, onError }: { itemKey: string; onError: (msg: string) => void }) {
  const levels = usePrioLevels();
  const cur = levels[itemKey] ?? 0;
  const pick = (level: number) => {
    prio.set(itemKey, level).catch((e) => onError(typeof e === "string" ? e : "Nie udało się zapisać flagi."));
    const d = document.activeElement?.closest("details");
    if (d) d.removeAttribute("open");
  };
  return (
    <details className="prio-menu">
      <summary aria-label="Flaga ważności" title="Flaga ważności (klawisze 1, 2, 3, a 0 usuwa)">
        {cur ? <PrioFlag level={cur} /> : <svg className="prio-flag" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path d={FLAG} fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" /></svg>}
      </summary>
      <div className="prio-pop" role="menu">
        {PRIOS.map((p) => (
          <button key={p.level} role="menuitemradio" aria-checked={cur === p.level} onClick={() => pick(p.level)}>
            <PrioFlag level={p.level} /> {p.label}{cur === p.level ? " ✓" : ""}
          </button>
        ))}
        {cur > 0 && <button role="menuitem" onClick={() => pick(0)}>Usuń flagę</button>}
      </div>
    </details>
  );
}

/** Przełącznik „flagi na górze listy”: oflagowane wiadomości (od bardzo ważnych) przed resztą także poza wyszukiwaniem. */
export function FlagsFirstToggle() {
  const on = useFlagsFirst();
  const t = on ? "Flagi na górze listy: włączone (kliknij, aby wrócić do układu wg daty)" : "Pokaż wiadomości z flagami na górze listy";
  return (
    <button className={"small flags-first" + (on ? " on" : "")} type="button" aria-pressed={on} title={t} aria-label={t}
      onClick={() => setFlagsFirst(!on)}><ActIcon n="flag" /></button>
  );
}
