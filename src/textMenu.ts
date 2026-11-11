import type { MenuEntry } from "./ContextMenu";

type Note = (text: string) => void;

const isEditable = (el: Element | null): el is HTMLInputElement | HTMLTextAreaElement | HTMLElement =>
  !!el && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el as HTMLElement).isContentEditable);

/** Polskie menu dla pól tekstowych, zaznaczonego tekstu i linków. Zwraca pustą listę, gdy nie ma czego pokazać. */
export function textMenuFor(target: EventTarget | null, note: Note): MenuEntry[] {
  const el = target instanceof Element ? target : null;
  const editable = el ? (el.closest("input, textarea, [contenteditable='true'], [contenteditable='']") as HTMLElement | null) : null;
  const field = editable && isEditable(editable) ? editable : null;
  const readOnly = field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement ? field.readOnly || field.disabled : false;
  const sel = field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement
    ? field.value.slice(field.selectionStart ?? 0, field.selectionEnd ?? 0)
    : window.getSelection()?.toString() ?? "";
  const link = el?.closest("a[href]") as HTMLAnchorElement | null;
  const out: MenuEntry[] = [];

  const copy = () => { try { document.execCommand("copy"); } catch { /* brak dostępu do schowka */ } };
  if (field) {
    if (!readOnly) out.push({ label: "Wytnij", icon: "eraser", disabled: !sel, onClick: () => { try { document.execCommand("cut"); } catch { /* */ } } });
    out.push({ label: "Kopiuj", icon: "copy", disabled: !sel, onClick: copy });
    if (!readOnly) {
      out.push({
        label: "Wklej", icon: "paste", onClick: () => {
          field.focus();
          navigator.clipboard.readText()
            .then((t) => { if (t) document.execCommand("insertText", false, t); })
            .catch(() => note("Nie mam dostępu do schowka. Wklej skrótem Ctrl+V."));
        },
      });
    }
    out.push("sep", {
      label: "Zaznacz wszystko", icon: "selectAll", onClick: () => {
        field.focus();
        if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) field.select();
        else document.execCommand("selectAll");
      },
    });
    return out;
  }
  if (link && /^(https?:|mailto:)/i.test(link.href)) {
    out.push({ label: "Kopiuj adres linku", icon: "link", onClick: () => { navigator.clipboard.writeText(link.href.replace(/^mailto:/i, "")).catch(() => note("Nie udało się skopiować.")); } });
  }
  if (sel.trim()) out.push({ label: "Kopiuj", icon: "copy", onClick: copy });
  return out;
}
