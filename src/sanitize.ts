// Oczyszczanie HTML tworzonego w edytorze (oraz wczytywanego z wersji roboczej).
// Zasada: dozwolona jest tylko niewielka lista znaczników; obrazy, style, skrypty, ramki
// i wszelkie atrybuty poza href w linkach są usuwane. DOMParser nie uruchamia skryptów.

const ALLOWED = new Set([
  "P", "BR", "DIV", "SPAN", "B", "STRONG", "I", "EM", "U", "S", "UL", "OL", "LI", "A", "BLOCKQUOTE",
]);
const DROP_WITH_CONTENT = new Set([
  "SCRIPT", "STYLE", "IFRAME", "OBJECT", "EMBED", "IMG", "SVG", "LINK", "META", "FORM", "INPUT",
  "BUTTON", "TEXTAREA", "SELECT", "VIDEO", "AUDIO", "CANVAS", "NOSCRIPT", "TEMPLATE", "HEAD", "TITLE",
]);

export function isSafeHref(href: string): boolean {
  return /^(https?:|mailto:)/i.test(href.trim());
}

function clean(node: Node, out: Node, doc: Document) {
  node.childNodes.forEach((child) => {
    if (child.nodeType === Node.TEXT_NODE) {
      out.appendChild(doc.createTextNode(child.textContent ?? ""));
      return;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) return;
    const el = child as Element;
    const tag = el.tagName.toUpperCase();
    if (DROP_WITH_CONTENT.has(tag)) return;
    if (!ALLOWED.has(tag)) {
      clean(el, out, doc); // nieznany znacznik: zachowujemy tylko jego zawartość
      return;
    }
    const copy = doc.createElement(tag.toLowerCase());
    if (tag === "A") {
      const href = el.getAttribute("href") ?? "";
      if (isSafeHref(href)) {
        copy.setAttribute("href", href.trim());
        copy.setAttribute("rel", "noopener noreferrer");
      }
    }
    if (tag === "BLOCKQUOTE") {
      copy.setAttribute("style", "margin:0 0 0 .8ex;border-left:2px solid #ccc;padding-left:1ex");
    }
    clean(el, copy, doc);
    out.appendChild(copy);
  });
}

export function sanitizeHtml(html: string): string {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const box = parsed.createElement("div");
  clean(parsed.body, box, parsed);
  return box.innerHTML;
}
