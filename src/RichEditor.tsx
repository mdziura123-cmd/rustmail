import { useEffect, useRef, useState } from "react";
import { isSafeHref, sanitizeHtml } from "./sanitize";

interface Props {
  initialHtml: string;
  onChange: (html: string) => void;
  disabled?: boolean;
}

/**
 * Prosty edytor tekstu sformatowanego (pogrubienie, kursywa, podkreślenie, listy, linki).
 * Wklejany tekst jest zawsze zwykłym tekstem, a upuszczanie plików/HTML jest blokowane.
 */
export function RichEditor({ initialHtml, onChange, disabled }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const savedRange = useRef<Range | null>(null);
  const [linkOpen, setLinkOpen] = useState(false);
  const [href, setHref] = useState("https://");
  const [linkErr, setLinkErr] = useState<string | null>(null);

  useEffect(() => {
    if (ref.current) {
      ref.current.innerHTML = sanitizeHtml(initialHtml);
      ref.current.focus();
      // kursor na początku (nad podpisem i cytatem)
      const sel = window.getSelection();
      if (sel) {
        const r = document.createRange();
        r.setStart(ref.current, 0);
        r.collapse(true);
        sel.removeAllRanges();
        sel.addRange(r);
      }
    }
    // treść początkowa jest ustawiana tylko raz
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const emit = () => onChange(ref.current?.innerHTML ?? "");

  const cmd = (name: string, value?: string) => {
    ref.current?.focus();
    document.execCommand(name, false, value);
    emit();
  };

  const rememberSelection = () => {
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0 && ref.current?.contains(sel.anchorNode)) {
      savedRange.current = sel.getRangeAt(0).cloneRange();
    }
  };

  const applyLink = () => {
    if (!isSafeHref(href) || href.trim().length < 8) {
      setLinkErr("Adres musi zaczynać się od https://, http:// lub mailto:");
      return;
    }
    ref.current?.focus();
    const sel = window.getSelection();
    if (sel && savedRange.current) {
      sel.removeAllRanges();
      sel.addRange(savedRange.current);
    }
    if (sel && sel.isCollapsed) {
      document.execCommand("insertHTML", false, `<a href="${href.replace(/"/g, "&quot;")}">${href.replace(/</g, "&lt;")}</a>`);
    } else {
      document.execCommand("createLink", false, href.trim());
    }
    setLinkOpen(false);
    setLinkErr(null);
    emit();
  };

  const onPaste = (e: React.ClipboardEvent) => {
    e.preventDefault();
    const text = e.clipboardData.getData("text/plain");
    if (text) document.execCommand("insertText", false, text);
    emit();
  };

  const btn = (label: string, title: string, run: () => void, cls = "") => (
    <button type="button" className={"tb " + cls} title={title} aria-label={title} disabled={disabled}
      onMouseDown={(e) => e.preventDefault()} onClick={run}>
      {label}
    </button>
  );

  return (
    <div className="editor">
      <div className="toolbar" role="toolbar" aria-label="Formatowanie">
        {btn("B", "Pogrubienie", () => cmd("bold"), "b")}
        {btn("I", "Kursywa", () => cmd("italic"), "i")}
        {btn("U", "Podkreślenie", () => cmd("underline"), "u")}
        {btn("• Lista", "Lista punktowana", () => cmd("insertUnorderedList"))}
        {btn("1. Lista", "Lista numerowana", () => cmd("insertOrderedList"))}
        {btn("Link", "Wstaw link", () => { rememberSelection(); setLinkOpen((v) => !v); })}
        {btn("Wyczyść", "Usuń formatowanie zaznaczenia", () => cmd("removeFormat"))}
      </div>
      {linkOpen && (
        <div className="linkbar">
          <input value={href} onChange={(e) => setHref(e.target.value)} aria-label="Adres linku"
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); applyLink(); } }} />
          <button type="button" className="small" onClick={applyLink}>Wstaw</button>
          <button type="button" className="small" onClick={() => { setLinkOpen(false); setLinkErr(null); }}>Anuluj</button>
          {linkErr && <span className="error">{linkErr}</span>}
        </div>
      )}
      <div ref={ref} className="editable" contentEditable={!disabled} suppressContentEditableWarning
        role="textbox" aria-multiline="true" aria-label="Treść wiadomości"
        onInput={emit} onPaste={onPaste} onDrop={(e) => e.preventDefault()} />
    </div>
  );
}
