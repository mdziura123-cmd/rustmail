// Małe animowane ikony przycisków w pasku akcji wiadomości. Animacja uruchamia się po najechaniu lub wejściu fokusem
// (patrz styles.css); przy włączonym w systemie ograniczeniu ruchu ikony stoją w miejscu.
const PATHS = {
  reply: "M9 14 4 9l5-5M4 9h10a6 6 0 0 1 6 6v3",
  replyAll: "M7 14 2 9l5-5M12 14 7 9l5-5M7 9h8a6 6 0 0 1 6 6v3",
  forward: "m15 14 5-5-5-5M20 9H10a6 6 0 0 0-6 6v3",
  archive: "M3 4h18v4H3zM5 8v12h14V8M10 12h4",
  inbox: "M3 13l3-8h12l3 8v6H3zM3 13h5l1 3h6l1-3h5",
  spam: "M12 3l9 16H3zM12 10v4M12 17h.01",
  notspam: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6zM9 12l2 2 4-4",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6",
  restore: "M3 12a9 9 0 1 0 3-6.7M3 4v5h5",
  read: "M3 9l9-6 9 6v11H3zM3 9l9 6 9-6",
  unread: "M3 6h18v12H3zM3 7l9 7 9-7",
  star: "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z",
  label: "M3 12V4h8l10 10-8 8zM7.5 7.5h.01",
  user: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8M4 21c0-4 4-6 8-6s8 2 8 6",
  edit: "M4 20h4L19 9l-4-4L4 16zM14 6l4 4",
  expand: "M7 9l5-5 5 5M7 15l5 5 5-5",
  selectAll: "M3 12l5 5L18 7M9 17l1.5 1.5L21 7",
  clear: "M6 6l12 12M18 6 6 18",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM21 21l-4.3-4.3",
  save: "M5 3h11l3 3v15H5zM8 3v5h7V3M8 21v-7h8v7",
  send: "M22 2 11 13M22 2l-7 20-4-9-9-4z",
  attach: "M21 11l-9 9a5 5 0 0 1-7-7l9-9a3.5 3.5 0 0 1 5 5l-9 9a2 2 0 0 1-3-3l8-8",
  list: "M9 6h12M9 12h12M9 18h12M4 6h.01M4 12h.01M4 18h.01",
  olist: "M10 6h11M10 12h11M10 18h11M4 4v4M3 8h2M3 14h2.5L3 17h3",
  link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
  eraser: "M20 20H9L4 15 14 5l6 6-7 7M8 11l6 6",
  plus: "M12 5v14M5 12h14",
  dict: "M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2zM12 8v6M9 11h6",
  calendar: "M4 6h16v14H4zM4 10h16M8 3v4M16 3v4",
  flag: "M5 21V4M5 4h11l-2 4 2 4H5",
  filter: "M4 5h16M7 12h10M10 19h4",
  collapse: "M7 5l5 5 5-5M7 19l5-5 5 5",
} as const;

export type ActIconName = keyof typeof PATHS;

export function ActIcon({ n, filled }: { n: ActIconName; filled?: boolean }) {
  return (
    <span className={"ai ai-" + n} aria-hidden="true">
      <svg viewBox="0 0 24 24" width="15" height="15" fill={filled ? "currentColor" : "none"} stroke="currentColor"
        strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d={PATHS[n]} />
      </svg>
    </span>
  );
}
