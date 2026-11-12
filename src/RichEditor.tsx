import { useEffect, useRef, useState } from "react";
import { ActIcon } from "./ActIcon";
import { EMOJI, token } from "./emoji";
import { FONTS, isSafeHref, SIZES, sanitizeHtml } from "./sanitize";

/** Operacje na treści edytora potrzebne do sprawdzania pisowni. */
export interface EditorApi {
  /** Zamienia całe słowo `from` na `to` w tekście edytora. */
  replaceWord: (from: string, to: string) => void;
  /** Podkreśla podane słowa (jeśli przeglądarka obsługuje CSS Custom Highlight API). */
  highlight: (words: string[]) => void;
  /** Wstawia zwykły tekst w miejscu kursora (np. szablon odpowiedzi). */
  insertText: (text: string) => void;
}

interface Props {
  initialHtml: string;
  onChange: (html: string) => void;
  disabled?: boolean;
  apiRef?: { current: EditorApi | null };
}

const WORD_CH = "\\p{L}\\p{M}\\p{N}_'’";
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const wordRe = (w: string) => new RegExp(`(?<![${WORD_CH}])${esc(w)}(?![${WORD_CH}])`, "gu");

type HighlightApi = { highlights: Map<string, unknown> };

/**
 * Prosty edytor tekstu sformatowanego (pogrubienie, kursywa, podkreślenie, listy, linki).
 * Wklejany tekst jest zawsze zwykłym tekstem, a upuszczanie plików/HTML jest blokowane.
 */
export function RichEditor({ initialHtml, onChange, disabled, apiRef }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const savedRange = useRef<Range | null>(null);
  const [linkOpen, setLinkOpen] = useState(false);
  const [href, setHref] = useState("https://");
  const [linkErr, setLinkErr] = useState<string | null>(null);
  const [emojiOpen, setEmojiOpen] = useState(false);

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

  useEffect(() => {
    if (!apiRef) return;
    const textNodes = () => {
      const out: Text[] = [];
      if (!ref.current) return out;
      const tw = document.createTreeWalker(ref.current, NodeFilter.SHOW_TEXT);
      for (let n = tw.nextNode(); n; n = tw.nextNode()) out.push(n as Text);
      return out;
    };
    apiRef.current = {
      replaceWord: (from, to) => {
        const re = wordRe(from);
        for (const t of textNodes()) {
          if (t.parentElement?.closest("blockquote")) continue;
          const v = t.nodeValue ?? "";
          re.lastIndex = 0;
          if (re.test(v)) { re.lastIndex = 0; t.nodeValue = v.replace(re, () => to); }
        }
        onChange(ref.current?.innerHTML ?? "");
      },
      insertText: (text) => {
        const el = ref.current;
        if (!el) return;
        el.focus();
        const sel = window.getSelection();
        if (sel && savedRange.current && el.contains(savedRange.current.startContainer)) {
          sel.removeAllRanges();
          sel.addRange(savedRange.current);
        }
        document.execCommand("insertText", false, text);
        onChange(el.innerHTML);
      },
      highlight: (words) => {
        const css = (typeof CSS !== "undefined" ? (CSS as unknown as HighlightApi) : null);
        const H = (window as unknown as { Highlight?: new (...r: Range[]) => unknown }).Highlight;
        if (!css || !css.highlights || !H) return;
        if (words.length === 0) { css.highlights.delete("rm-spell"); return; }
        const ranges: Range[] = [];
        for (const w of words) {
          const re = wordRe(w);
          for (const t of textNodes()) {
            if (t.parentElement?.closest("blockquote")) continue;
            for (const m of (t.nodeValue ?? "").matchAll(re)) {
              const r = document.createRange();
              r.setStart(t, m.index ?? 0);
              r.setEnd(t, (m.index ?? 0) + m[0].length);
              ranges.push(r);
            }
          }
        }
        css.highlights.set("rm-spell", new H(...ranges));
      },
    };
    return () => {
      apiRef.current = null;
      const css = (typeof CSS !== "undefined" ? (CSS as unknown as HighlightApi) : null);
      css?.highlights?.delete("rm-spell");
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

  // Lista rozwijana odbiera fokus edytorowi, więc zaznaczenie zapamiętujemy i przywracamy.
  const applyWithSelection = (name: string, value: string) => {
    if (!value) return;
    ref.current?.focus();
    const sel = window.getSelection();
    if (sel && savedRange.current) {
      sel.removeAllRanges();
      sel.addRange(savedRange.current);
    }
    document.execCommand(name, false, value);
    emit();
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

  const btn = (label: React.ReactNode, title: string, run: () => void, cls = "") => (
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
        <select className="tb-select" aria-label="Czcionka" disabled={disabled} value=""
          onMouseDown={rememberSelection} onFocus={rememberSelection}
          onChange={(e) => applyWithSelection("fontName", e.target.value)}>
          <option value="">Czcionka</option>
          {FONTS.map((f) => <option key={f} value={f} style={{ fontFamily: f }}>{f}</option>)}
        </select>
        <select className="tb-select" aria-label="Rozmiar czcionki" disabled={disabled} value=""
          onMouseDown={rememberSelection} onFocus={rememberSelection}
          onChange={(e) => applyWithSelection("fontSize", e.target.value)}>
          <option value="">Rozmiar</option>
          {SIZES.map((s) => <option key={s.value} value={s.value}>{s.label}</option>)}
        </select>
        {btn(<ActIcon n="list" />, "Lista punktowana", () => cmd("insertUnorderedList"))}
        {btn(<ActIcon n="olist" />, "Lista numerowana", () => cmd("insertOrderedList"))}
        {btn(<ActIcon n="link" />, "Wstaw link", () => { rememberSelection(); setLinkOpen((v) => !v); })}
        {btn(<ActIcon n="eraser" />, "Usuń formatowanie zaznaczenia", () => cmd("removeFormat"))}
        {btn(
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
            <circle cx="12" cy="12" r="9" /><path d="M8.5 14.5c1 1.4 2.2 2 3.5 2s2.5-.6 3.5-2" /><path d="M9 9.5v.5M15 9.5v.5" />
          </svg>,
          "Wstaw emotkę", () => { rememberSelection(); setEmojiOpen((v) => !v); })}
      </div>
      {emojiOpen && (
        <div className="emoji-pop" role="listbox" aria-label="Emotki">
          <p className="hint">W oknie pisania widać opis, np. [uśmiech]; odbiorca dostanie prawdziwą emotkę.</p>
          <div className="emoji-grid">
            {EMOJI.map((e) => (
              <button key={e.name} type="button" className="small" disabled={disabled}
                onMouseDown={(ev) => ev.preventDefault()}
                onClick={() => { apiRef?.current?.insertText(token(e.name)); }}>
                {e.name}
              </button>
            ))}
          </div>
          <button type="button" className="small" onClick={() => setEmojiOpen(false)}>Zamknij</button>
        </div>
      )}
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
        onInput={emit} onBlur={rememberSelection} onPaste={onPaste} onDrop={(e) => e.preventDefault()} />
    </div>
  );
}
