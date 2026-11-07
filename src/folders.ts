export type FolderId =
  | "inbox" | "unread" | "sent" | "drafts" | "spam"
  | "suspicious" | "trash" | "archive" | "starred" | "important";

export interface Folder {
  id: FolderId;
  label: string;
  icon: string; // dane ścieżki SVG (viewBox 24x24)
}

export const FOLDERS: Folder[] = [
  { id: "inbox", label: "Odebrane", icon: "M3 13l3-8h12l3 8v6H3z M3 13h5l1 2h6l1-2h5" },
  { id: "unread", label: "Nieprzeczytane", icon: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z" },
  { id: "sent", label: "Wysłane", icon: "M3 11l18-8-8 18-2-8z" },
  { id: "drafts", label: "Wersje robocze", icon: "M4 20h4L20 8l-4-4L4 16z" },
  { id: "spam", label: "Spam", icon: "M12 3l9 16H3z M12 10v4 M12 17v.01" },
  { id: "suspicious", label: "Podejrzane", icon: "M12 3l8 3v6c0 5-4 8-8 9-4-1-8-4-8-9V6z M12 8v5 M12 16v.01" },
  { id: "trash", label: "Kosz", icon: "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13" },
  { id: "archive", label: "Archiwum", icon: "M3 4h18v4H3z M5 8v12h14V8 M10 12h4" },
  { id: "starred", label: "Oznaczone gwiazdką", icon: "M12 3l2.9 6 6.6.9-4.8 4.6 1.2 6.5L12 17.8 6.1 21l1.2-6.5L2.5 9.9 9.1 9z" },
  { id: "important", label: "Ważne", icon: "M5 3v18 M5 4h13l-3 4 3 4H5" },
];
