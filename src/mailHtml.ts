// Czyszczenie HTML wiadomości odebranych (niezaufanych) przed wyświetleniem.
// Budujemy NOWE drzewo DOM wyłącznie z dozwolonych elementów i atrybutów (lista dozwolonych, nie zabronionych).
// DOMParser nie uruchamia skryptów ani nie pobiera zasobów. Zdalne obrazy są blokowane, dopóki
// użytkownik się nie zgodzi; linki nie mają atrybutu href (obsługuje je aplikacja po potwierdzeniu).

const ALLOWED = new Set([
  "A", "ABBR", "B", "BLOCKQUOTE", "BR", "CAPTION", "CENTER", "CODE", "DIV", "EM", "FONT", "H1", "H2", "H3", "H4",
  "H5", "H6", "HR", "I", "IMG", "LI", "OL", "P", "PRE", "S", "SMALL", "SPAN", "STRIKE", "STRONG", "SUB", "SUP",
  "TABLE", "TBODY", "TD", "TFOOT", "TH", "THEAD", "TR", "U", "UL",
]);
const DROP = new Set([
  "SCRIPT", "STYLE", "IFRAME", "OBJECT", "EMBED", "SVG", "LINK", "META", "FORM", "INPUT", "BUTTON", "TEXTAREA",
  "SELECT", "VIDEO", "AUDIO", "CANVAS", "NOSCRIPT", "TEMPLATE", "HEAD", "TITLE", "MATH", "BASE", "APPLET",
  "FRAME", "FRAMESET", "SOURCE", "TRACK", "PICTURE", "DIALOG",
]);
const MAX_NODES = 20000;
const MAX_DEPTH = 60;

const COLOR = /^(#[0-9a-f]{3,8}|rgba?\([\d\s,.%/]+\)|hsla?\([\d\s,.%/deg]+\)|[a-z]{3,20})$/i;
const LEN = /^\d{1,4}(\.\d+)?(px|pt|em|rem|%)?$/i;
const FONT_FAMILY = /^[\w\s,'"-]{1,120}$/;

function lengthOk(v: string, maxPx: number): boolean {
  if (!LEN.test(v)) return false;
  const n = parseFloat(v);
  const unit = v.replace(/^[\d.]+/, "").toLowerCase();
  if (unit === "%") return n <= 100;
  if (unit === "em" || unit === "rem") return n <= maxPx / 8;
  return n <= maxPx;
}
function boxOk(v: string, maxPx: number): boolean {
  const parts = v.trim().split(/\s+/);
  return parts.length >= 1 && parts.length <= 4 && parts.every((p) => lengthOk(p, maxPx));
}

type Check = (v: string) => boolean;
const STYLE_RULES: Record<string, Check> = {
  "color": (v) => COLOR.test(v),
  "background-color": (v) => COLOR.test(v),
  "font-size": (v) => lengthOk(v, 72),
  "font-weight": (v) => /^(normal|bold|bolder|lighter|[1-9]00)$/i.test(v),
  "font-style": (v) => /^(normal|italic|oblique)$/i.test(v),
  "font-family": (v) => FONT_FAMILY.test(v),
  "text-decoration": (v) => /^(none|underline|line-through|overline)( (none|underline|line-through|overline))*$/i.test(v),
  "text-align": (v) => /^(left|right|center|justify)$/i.test(v),
  "vertical-align": (v) => /^(top|middle|bottom|baseline)$/i.test(v),
  "line-height": (v) => /^(normal|\d{1,2}(\.\d+)?)$/i.test(v) || lengthOk(v, 200),
  "white-space": (v) => /^(normal|nowrap|pre|pre-wrap|pre-line)$/i.test(v),
  "border-collapse": (v) => /^(collapse|separate)$/i.test(v),
  "width": (v) => lengthOk(v, 1200),
  "max-width": (v) => lengthOk(v, 1200),
  "height": (v) => lengthOk(v, 1200),
  "min-height": (v) => lengthOk(v, 600),
  "border-radius": (v) => boxOk(v, 100),
  "padding": (v) => boxOk(v, 200), "padding-top": (v) => boxOk(v, 200), "padding-right": (v) => boxOk(v, 200),
  "padding-bottom": (v) => boxOk(v, 200), "padding-left": (v) => boxOk(v, 200),
  "margin": (v) => boxOk(v, 200), "margin-top": (v) => boxOk(v, 200), "margin-right": (v) => boxOk(v, 200),
  "margin-bottom": (v) => boxOk(v, 200), "margin-left": (v) => boxOk(v, 200),
  "border": borderOk, "border-top": borderOk, "border-right": borderOk, "border-bottom": borderOk, "border-left": borderOk,
};
function borderOk(v: string): boolean {
  return /^[\w\s#.,()%/-]{1,80}$/.test(v) && !/url|expression/i.test(v);
}
const STYLE_BAD = /url\s*\(|expression|javascript|@import|\\|\/\*|var\s*\(|attr\s*\(|image-set|-moz-binding|behavior/i;

/** Zostawia tylko bezpieczne właściwości wyglądu. Nie dopuszcza position, float, z-index, tła-obrazu itp. */
export function cleanStyle(style: string): string {
  const out: string[] = [];
  for (const decl of style.split(";")) {
    const i = decl.indexOf(":");
    if (i < 0) continue;
    const prop = decl.slice(0, i).trim().toLowerCase();
    const value = decl.slice(i + 1).replace(/!important/i, "").trim();
    if (!value || STYLE_BAD.test(value)) continue;
    const rule = STYLE_RULES[prop];
    if (rule && rule(value)) out.push(`${prop}:${value}`);
  }
  return out.join(";");
}

export interface SanitizedMail {
  /** Czy w wiadomości wykryto i (przy hideQuotes) ukryto cytowaną część. */
  hadQuote: boolean;
  html: string;
  /** Liczba zdalnych obrazów (http/https) w wiadomości. */
  remoteImages: number;
}

const DATA_IMG = /^data:image\/(png|jpeg|gif|webp);base64,[a-z0-9+/=\s]+$/i;
const NUM = /^\d{1,4}%?$/;

function copyAttrs(el: Element, copy: HTMLElement, tag: string) {
  const get = (n: string) => (el.getAttribute(n) ?? "").trim();
  const style: string[] = [];
  const st = cleanStyle(get("style"));
  if (st) style.push(st);
  const bg = get("bgcolor");
  if (bg && COLOR.test(bg)) style.push(`background-color:${bg}`);
  if (style.length) copy.setAttribute("style", style.join(";"));

  const title = get("title");
  if (title) copy.setAttribute("title", title.slice(0, 200));
  if (/^(ltr|rtl)$/i.test(get("dir"))) copy.setAttribute("dir", get("dir").toLowerCase());
  if (/^(left|right|center|justify)$/i.test(get("align"))) copy.setAttribute("align", get("align").toLowerCase());
  if (/^(top|middle|bottom|baseline)$/i.test(get("valign"))) copy.setAttribute("valign", get("valign").toLowerCase());

  if (["TABLE", "TD", "TH", "IMG"].includes(tag)) {
    for (const a of ["width", "height"]) if (NUM.test(get(a))) copy.setAttribute(a, get(a));
  }
  if (tag === "TABLE") {
    for (const a of ["border", "cellpadding", "cellspacing"]) if (/^\d{1,2}$/.test(get(a))) copy.setAttribute(a, get(a));
  }
  if (tag === "TD" || tag === "TH") {
    for (const a of ["colspan", "rowspan"]) if (/^\d{1,2}$/.test(get(a)) && Number(get(a)) >= 1) copy.setAttribute(a, get(a));
  }
  if (tag === "FONT") {
    if (COLOR.test(get("color"))) copy.setAttribute("color", get("color"));
    if (/^[1-7]$/.test(get("size"))) copy.setAttribute("size", get("size"));
    if (FONT_FAMILY.test(get("face"))) copy.setAttribute("face", get("face"));
  }
}

const QUOTE_SELECTOR =
  ".gmail_quote, .gmail_extra, blockquote[type=cite], .yahoo_quoted, #divRplyFwdMsg, #appendonsend, #mail-editor-reference-message-container";

export function sanitizeMail(html: string, allowImages: boolean, hideQuotes = false): SanitizedMail {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const doc = parsed;
  const hadQuote = parsed.body.querySelector(QUOTE_SELECTOR) !== null;
  if (hideQuotes && hadQuote) {
    // ukrywamy cytat tylko wtedy, gdy zostaje jakaś własna treść wiadomości
    const probe = parsed.body.cloneNode(true) as HTMLElement;
    probe.querySelectorAll(QUOTE_SELECTOR).forEach((e) => e.remove());
    if ((probe.textContent ?? "").trim()) parsed.body.querySelectorAll(QUOTE_SELECTOR).forEach((e) => e.remove());
  }
  const box = doc.createElement("div");
  let nodes = 0;
  let remote = 0;

  const walk = (src: Node, dst: Node, depth: number) => {
    src.childNodes.forEach((child) => {
      if (++nodes > MAX_NODES || depth > MAX_DEPTH) return;
      if (child.nodeType === Node.TEXT_NODE) {
        dst.appendChild(doc.createTextNode(child.textContent ?? ""));
        return;
      }
      if (child.nodeType !== Node.ELEMENT_NODE) return;
      const el = child as Element;
      const tag = el.tagName.toUpperCase();
      if (DROP.has(tag)) return;
      if (!ALLOWED.has(tag)) {
        walk(el, dst, depth + 1); // nieznany znacznik: zachowujemy tylko zawartość
        return;
      }
      if (tag === "IMG") {
        const src2 = (el.getAttribute("src") ?? "").trim();
        const alt = (el.getAttribute("alt") ?? "").trim().slice(0, 120);
        if (DATA_IMG.test(src2)) {
          const img = doc.createElement("img");
          img.setAttribute("src", src2.replace(/\s+/g, ""));
          img.setAttribute("alt", alt);
          copyAttrs(el, img, tag);
          dst.appendChild(img);
        } else if (/^https?:\/\//i.test(src2)) {
          remote++;
          if (allowImages && /^https:\/\//i.test(src2)) {
            const img = doc.createElement("img");
            img.setAttribute("data-rm-src", src2);
            img.setAttribute("alt", alt);
            copyAttrs(el, img, tag);
            dst.appendChild(img);
          } else {
            const ph = doc.createElement("span");
            ph.className = "img-blocked";
            ph.textContent = alt ? `[obraz: ${alt}]` : "[obraz zablokowany]";
            dst.appendChild(ph);
          }
        } else {
          const ph = doc.createElement("span");
          ph.className = "img-blocked";
          ph.textContent = alt ? `[obraz: ${alt}]` : "[obraz wbudowany – niewyświetlany]";
          dst.appendChild(ph);
        }
        return;
      }
      const copy = doc.createElement(tag.toLowerCase());
      copyAttrs(el, copy, tag);
      if (tag === "A") {
        const href = (el.getAttribute("href") ?? "").trim();
        if (/^(https?:|mailto:)/i.test(href) && href.length <= 2000 && !/[\s\u0000-\u001f]/.test(href)) {
          copy.setAttribute("data-href", href);
          copy.setAttribute("class", "mail-link");
          copy.setAttribute("role", "link");
          copy.setAttribute("tabindex", "0");
        }
      }
      walk(el, copy, depth + 1);
      dst.appendChild(copy);
    });
  };
  walk(parsed.body, box, 0);
  return { html: box.innerHTML, remoteImages: remote, hadQuote };
}
