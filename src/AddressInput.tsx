import { useEffect, useRef, useState } from "react";
import { api } from "./api";

interface Props {
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
  placeholder?: string;
  label: string;
}

interface Sug { email: string; name: string }

/** Zamienia „Jan Kowalski” + adres na tekst do pola (nazwa w cudzysłowie, gdy zawiera znaki specjalne). */
function format(s: Sug): string {
  if (!s.name) return s.email;
  const n = /[",;<>()]/.test(s.name) ? `"${s.name.replace(/["\\]/g, "")}"` : s.name;
  return `${n} <${s.email}>`;
}

/** Pole adresów z podpowiedziami z książki adresów (adresy, do których już pisałeś). */
export function AddressInput({ value, onChange, disabled, placeholder, label }: Props) {
  const [sugs, setSugs] = useState<Sug[]>([]);
  const [idx, setIdx] = useState(0);
  const [open, setOpen] = useState(false);
  const seq = useRef(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const cut = Math.max(value.lastIndexOf(","), value.lastIndexOf(";"));
  const token = value.slice(cut + 1).trim();

  useEffect(() => {
    if (disabled || token.length < 1 || token.includes("<")) { setSugs([]); return; }
    const my = ++seq.current;
    const t = window.setTimeout(() => {
      api.contactsSuggest(token).then((r) => {
        if (my !== seq.current) return;
        // nie podpowiadamy adresu, który jest już w całości wpisany
        setSugs(r.filter((x) => x.email !== token.toLowerCase()));
        setIdx(0);
      }).catch(() => { if (my === seq.current) setSugs([]); });
    }, 120);
    return () => window.clearTimeout(t);
  }, [token, disabled]);

  const pick = (s: Sug) => {
    const head = cut >= 0 ? value.slice(0, cut + 1).trimEnd() + " " : "";
    onChange(head + format(s) + ", ");
    setSugs([]); setOpen(false);
    inputRef.current?.focus();
  };

  const show = open && sugs.length > 0;
  return (
    <div className="addr-wrap">
      <input ref={inputRef} value={value} disabled={disabled} placeholder={placeholder} aria-label={label}
        role="combobox" aria-expanded={show} aria-autocomplete="list" autoComplete="off" spellCheck={false}
        onChange={(e) => { onChange(e.target.value); setOpen(true); }}
        onFocus={() => setOpen(true)}
        onBlur={() => window.setTimeout(() => setOpen(false), 120)}
        onKeyDown={(e) => {
          if (!show) return;
          if (e.key === "ArrowDown") { e.preventDefault(); setIdx((i) => (i + 1) % sugs.length); }
          else if (e.key === "ArrowUp") { e.preventDefault(); setIdx((i) => (i <= 0 ? sugs.length - 1 : i - 1)); }
          else if (e.key === "Enter" || e.key === "Tab") { e.preventDefault(); pick(sugs[idx]); }
          else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setOpen(false); }
        }} />
      {show && (
        <div className="addr-pop" role="listbox">
          {sugs.map((s, i) => (
            <div key={s.email} role="option" aria-selected={i === idx} className={"addr-opt" + (i === idx ? " on" : "")}
              onMouseDown={(e) => { e.preventDefault(); pick(s); }} onMouseEnter={() => setIdx(i)}>
              <span className="addr-name">{s.name || s.email}</span>
              {s.name && <span className="addr-mail">{s.email}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
