// Czyszczenie HTML wiadomości odebranych (niezaufanych) przed wyświetleniem.
// Budujemy NOWE drzewo DOM wyłącznie z dozwolonych elementów i atrybutów (lista dozwolonych, nie zabronionych).
// DOMParser nie uruchamia skryptów ani nie pobiera zasobów. Zdalne obrazy są blokowane, dopóki
// użytkownik się nie zgodzi; linki nie mają atrybutu href (obsługuje je aplikacja po potwierdzeniu).
import { emojiNodes } from "./emoji";

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
  /** Liczba zablokowanych pikseli śledzących (blokowane zawsze, także po zgodzie na obrazy). */
  trackers: number;
}

// ---------- Ochrona przed śledzeniem ----------
// Piksel śledzący to zwykle obraz 1x1 (lub ukryty) z adresem unikalnym dla odbiorcy. Pobranie go
// mówi nadawcy, kiedy, skąd (adres IP) i na jakim urządzeniu otwarto wiadomość.
const TRACKER_HOSTS = [
  "mailchimp.com", "list-manage.com", "mandrillapp.com", "sendgrid.net", "sendgrid.com", "mailgun.org",
  "mailgun.com", "sparkpostmail.com", "sparkpost.com", "hubspot.com", "hs-analytics.net", "hubspotemail.net",
  "hsforms.com", "mktoweb.com", "marketo.com", "mkt-track.com", "pardot.com", "salesforce.com", "exacttarget.com",
  "returnpath.net", "250ok.com", "litmus.com", "emltrk.com", "mailtrack.io", "mailtrack.com", "yesware.com",
  "streak.com", "mixmax.com", "superhuman.com", "bananatag.com", "getnotify.com", "readnotify.com",
  "mailspring.com", "cmail19.com", "cmail20.com", "createsend.com", "campaign-archive.com", "sendinblue.com",
  "brevo.com", "sibautomation.com", "klaviyo.com", "klaviyomail.com", "customer.io", "customeriomail.com",
  "intercom-mail.com", "intercom.io", "braze.com", "appboy.com", "iterable.com", "links.iterable.com",
  "convertkit.com", "ck.page", "mailerlite.com", "mlsend.com", "getresponse.com", "gr-cdn.com", "freshmail.com",
  "freshmail.pl", "smsapi.pl", "salesmanago.com", "salesmanago.pl", "edrone.me", "mailjet.com", "mjt.lu",
  "constantcontact.com", "rs6.net", "campaignmonitor.com", "emarsys.net", "emarsys.com", "bluecore.com",
  "doubleclick.net", "googleadservices.com", "google-analytics.com", "facebook.com", "facebook.net",
  "linkedin.com", "licdn.com", "twitter.com", "ads-twitter.com", "t.co", "bat.bing.com", "adform.net",
  "criteo.com", "criteo.net", "rubiconproject.com", "adsrvr.org", "scorecardresearch.com", "mathtag.com",
  "tracking.", "track.", "pixel.", "beacon.", "open.", "email.mg.", "trk.",
];
const TRACKER_PATH = /(\/|[._-])(track(ing)?|pixel|beacon|open(ed)?(\.gif|\.png|\.jpg|\/)|wf\/open|e\/o\/|oo\.gif|o\.gif|trk|impression|imp\.gif|1x1|spacer\.gif)/i;

function hostOf(u: string): string {
  try { return new URL(u).hostname.toLowerCase(); } catch { return ""; }
}
function hostMatches(host: string): boolean {
  return TRACKER_HOSTS.some((t) =>
    t.endsWith(".") ? host.startsWith(t) : host === t || host.endsWith("." + t));
}
const px = (v: string | null | undefined): number | null => {
  const m = /^\s*(\d+(?:\.\d+)?)\s*(px)?\s*$/i.exec(v ?? "");
  return m ? Number(m[1]) : null;
};
function styleSize(style: string, prop: "width" | "height"): number | null {
  const m = new RegExp("(?:^|;)\\s*" + prop + "\\s*:\\s*([^;]+)", "i").exec(style);
  return m ? px(m[1]) : null;
}
/** Czy obraz zdalny wygląda na piksel śledzący (rozmiar ≤ 2 px, ukryty albo znany adres śledzący). */
export function isTrackingImage(el: Element, src: string): boolean {
  const style = el.getAttribute("style") ?? "";
  if (/display\s*:\s*none|visibility\s*:\s*hidden|opacity\s*:\s*0(?![.\d])/i.test(style)) return true;
  const w = px(el.getAttribute("width")) ?? styleSize(style, "width");
  const h = px(el.getAttribute("height")) ?? styleSize(style, "height");
  if ((w !== null && w <= 2) || (h !== null && h <= 2)) return true;
  if (hostMatches(hostOf(src))) {
    // znany serwis wysyłkowy: blokujemy tylko to, co wygląda na licznik (nie logo czy zdjęcia produktów)
    return TRACKER_PATH.test(src) || (w === null && h === null && /[?&](u|uid|e|em|email|rid|r|id|mid|cid)=/i.test(src));
  }
  return TRACKER_PATH.test(new URL(src, "https://x.invalid").pathname) && w === null && h === null;
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
  let trackers = 0;

  const walk = (src: Node, dst: Node, depth: number) => {
    src.childNodes.forEach((child) => {
      if (++nodes > MAX_NODES || depth > MAX_DEPTH) return;
      if (child.nodeType === Node.TEXT_NODE) {
        for (const n of emojiNodes(child.textContent ?? "", doc)) dst.appendChild(n);
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
        } else if (/^https?:\/\//i.test(src2) && isTrackingImage(el, src2)) {
          trackers++; // nigdy nie pobieramy – nawet po zgodzie na obrazy
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
  return { html: box.innerHTML, remoteImages: remote, hadQuote, trackers };
}
