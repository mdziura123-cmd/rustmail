import { ActIcon } from "./ActIcon";
import { EMPTY_FILTERS, FLAG_OPTIONS, hasFilters, type FlagFilter, type Filters } from "./searchQuery";

interface Props {
  value: Filters;
  onChange: (f: Filters) => void;
  error?: string | null;
}

/** Dodatkowe opcje wyszukiwania pod polem „Szukaj”: nadawca, temat, zakres dat i flaga. */
export function SearchFilters({ value, onChange, error }: Props) {
  const set = (p: Partial<Filters>) => onChange({ ...value, ...p });
  return (
    <div className="search-opts" role="group" aria-label="Opcje wyszukiwania">
      <label><span><ActIcon n="user" />Nadawca</span>
        <input value={value.from} maxLength={100} placeholder="np. Ala albo ala@firma.pl" onChange={(e) => set({ from: e.target.value })} /></label>
      <label><span><ActIcon n="edit" />Temat</span>
        <input value={value.subject} maxLength={100} placeholder="słowa z tematu" onChange={(e) => set({ subject: e.target.value })} /></label>
      <label><span><ActIcon n="calendar" />Data od</span>
        <input type="date" value={value.dateFrom} onChange={(e) => set({ dateFrom: e.target.value })} /></label>
      <label><span><ActIcon n="calendar" />Data do</span>
        <input type="date" value={value.dateTo} onChange={(e) => set({ dateTo: e.target.value })} /></label>
      <label><span><ActIcon n="flag" />Flaga</span>
        <select value={value.flag} onChange={(e) => set({ flag: e.target.value as FlagFilter })}>
          {FLAG_OPTIONS.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
        </select></label>
      <button type="button" className="small" disabled={!hasFilters(value)} title="Wyczyść opcje" aria-label="Wyczyść opcje"
        onClick={() => onChange(EMPTY_FILTERS)}><ActIcon n="clear" /></button>
      {error && <p className="error" role="alert">{error}</p>}
      <p className="hint">Wyniki z flagami są na górze (bardzo ważne, ważne, mniej ważne), reszta od najnowszych.</p>
    </div>
  );
}
