import { useEffect, useRef } from "react";
import { ActIcon } from "./ActIcon";
import { SearchFilters } from "./SearchFilters";
import { hasFilters, type Filters } from "./searchQuery";

interface Props {
  query: string;
  onQuery: (q: string) => void;
  filters: Filters;
  onFilters: (f: Filters) => void;
  error: string | null;
  open: boolean;
  onOpen: (v: boolean) => void;
  onSubmit: () => void;
  onClear: () => void;
  /** Czy trwa wyszukiwanie (wtedy widać przycisk czyszczenia). */
  active: boolean;
  placeholder: string;
}

/** Pole wyszukiwania na środku paska okna, z rozwijanymi opcjami (nadawca, temat, daty, flaga). */
export function TitleSearch(p: Props) {
  const box = useRef<HTMLFormElement>(null);
  useEffect(() => {
    if (!p.open) return;
    const away = (e: MouseEvent) => { if (box.current && !box.current.contains(e.target as Node)) p.onOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") p.onOpen(false); };
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => { window.removeEventListener("mousedown", away); window.removeEventListener("keydown", esc); };
  }, [p.open, p.onOpen]);
  return (
    <form className="tb-search" role="search" ref={box} onSubmit={(e) => { e.preventDefault(); p.onSubmit(); }}>
      <div className="tb-search-row">
        <span className="tb-search-ico"><ActIcon n="search" /></span>
        <input id="main-search" type="search" value={p.query} maxLength={400} placeholder={p.placeholder} aria-label="Szukaj"
          onFocus={() => p.onOpen(true)} onChange={(e) => p.onQuery(e.target.value)} />
        <button type="button" className="tb-sbtn" aria-expanded={p.open} title="Opcje wyszukiwania: nadawca, temat, daty, flaga" aria-label="Opcje wyszukiwania"
          onClick={() => p.onOpen(!p.open)}><ActIcon n="filter" />{hasFilters(p.filters) && <span className="dot" aria-hidden="true" />}</button>
        {p.active && <button type="button" className="tb-sbtn" title="Wyczyść wyszukiwanie" aria-label="Wyczyść wyszukiwanie" onClick={p.onClear}><ActIcon n="clear" /></button>}
        <button type="submit" className="tb-sbtn go" title="Szukaj" aria-label="Szukaj"><ActIcon n="search" /></button>
      </div>
      {p.open && (
        <div className="tb-search-pop">
          <SearchFilters value={p.filters} onChange={p.onFilters} error={p.error} />
        </div>
      )}
    </form>
  );
}
