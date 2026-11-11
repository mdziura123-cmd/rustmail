import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ActIcon, type ActIconName } from "./ActIcon";

export interface MenuItem {
  label: string;
  icon?: ActIconName;
  /** Kolorowa kropka zamiast ikony (np. flagi). */
  swatch?: string;
  checked?: boolean;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
}
export type MenuEntry = MenuItem | "sep";

interface Open { x: number; y: number; items: MenuEntry[] }

/** Własne menu kontekstowe po polsku (zamiast domyślnego, angielskiego menu przeglądarki). */
export function ContextMenu({ x, y, items, onClose }: Open & { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
  }, [x, y, items]);

  useEffect(() => {
    const away = (e: Event) => { if (ref.current && !ref.current.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "Tab") { e.preventDefault(); onClose(); return; }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
      e.preventDefault();
      const btns = [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      if (btns.length === 0) return;
      const i = btns.indexOf(document.activeElement as HTMLButtonElement);
      const n = e.key === "Home" ? 0 : e.key === "End" ? btns.length - 1
        : e.key === "ArrowDown" ? (i + 1) % btns.length : (i <= 0 ? btns.length - 1 : i - 1);
      btns[n].focus();
    };
    window.addEventListener("mousedown", away, true);
    window.addEventListener("touchstart", away, true);
    window.addEventListener("contextmenu", away, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("resize", onClose);
    window.addEventListener("blur", onClose);
    window.addEventListener("scroll", onClose, true);
    return () => {
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("touchstart", away, true);
      window.removeEventListener("contextmenu", away, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("scroll", onClose, true);
    };
  }, [onClose]);

  return (
    <div className="ctx-menu" role="menu" ref={ref} style={pos} onContextMenu={(e) => e.preventDefault()}>
      {items.map((it, i) => it === "sep" ? <div key={i} className="ctx-sep" role="separator" /> : (
        <button key={i} role="menuitem" disabled={it.disabled} className={"ctx-item" + (it.danger ? " danger" : "")}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => { onClose(); it.onClick(); }}>
          <span className="ctx-ico">
            {it.swatch ? <span className="ctx-swatch" style={{ background: it.swatch }} /> : it.icon ? <ActIcon n={it.icon} /> : null}
          </span>
          <span className="ctx-label">{it.label}</span>
          {it.checked && <span className="ctx-check" aria-hidden="true">✓</span>}
        </button>
      ))}
    </div>
  );
}

/** Obsługa menu: prawy przycisk myszy oraz długie przytrzymanie palca (ekran dotykowy). */
export function useContextMenu() {
  const [open, setOpen] = useState<Open | null>(null);
  const close = useCallback(() => setOpen(null), []);
  const timer = useRef(0);
  const start = useRef<{ x: number; y: number } | null>(null);
  const longPressed = useRef(false);

  const show = useCallback((x: number, y: number, items: MenuEntry[]) => {
    if (items.length > 0) setOpen({ x, y, items });
  }, []);

  /** Właściwości do rozłożenia na elemencie wiersza: `{...bind(() => items)}`. */
  const bind = (getItems: () => MenuEntry[]) => ({
    onContextMenu: (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      show(e.clientX, e.clientY, getItems());
    },
    onPointerDown: (e: React.PointerEvent) => {
      if (e.pointerType !== "touch") return;
      longPressed.current = false;
      start.current = { x: e.clientX, y: e.clientY };
      const { clientX: x, clientY: y } = e;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => { longPressed.current = true; show(x, y, getItems()); }, 550);
    },
    onPointerMove: (e: React.PointerEvent) => {
      const s = start.current;
      if (s && Math.hypot(e.clientX - s.x, e.clientY - s.y) > 10) window.clearTimeout(timer.current);
    },
    onPointerUp: () => window.clearTimeout(timer.current),
    onPointerCancel: () => window.clearTimeout(timer.current),
    // Po długim przytrzymaniu puszczenie palca nie ma otwierać wiadomości.
    onClickCapture: (e: React.MouseEvent) => {
      if (longPressed.current) { longPressed.current = false; e.preventDefault(); e.stopPropagation(); }
    },
  });

  const menu = open ? <ContextMenu {...open} onClose={close} /> : null;
  return { menu, bind, show, close };
}
