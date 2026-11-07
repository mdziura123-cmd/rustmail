// WYŁĄCZNIE do oceny układu graficznego w etapie 1.
// To nie są dane z Gmaila. Zostanie usunięte po podłączeniu OAuth (etap 2).
import type { Thread } from "./types";

export const PREVIEW_THREADS: Thread[] = [
  {
    id: "preview-1",
    subject: "Termin spotkania w przyszłym tygodniu",
    participants: "Anna Przykładowa, ja",
    snippet: "Środa o 10:00 będzie idealnie, wyślę zaproszenie.",
    date: "10:42",
    unread: true,
    starred: true,
    messages: [
      { id: "m1", from: "Anna Przykładowa", mine: false, date: "wt., 09:12",
        body: "Cześć! Czy możemy umówić się na krótkie spotkanie w przyszłym tygodniu? Chciałabym omówić plan wdrożenia." },
      { id: "m2", from: "Ja", mine: true, date: "wt., 09:40",
        body: "Jasne. Pasuje mi wtorek po południu albo środa rano. Jaki termin jest dla Ciebie wygodniejszy?" },
      { id: "m3", from: "Anna Przykładowa", mine: false, date: "wt., 10:42",
        body: "Środa o 10:00 będzie idealnie, wyślę zaproszenie. W załączniku wstępna agenda.",
        attachments: ["agenda.pdf (84 KB)"] },
    ],
  },
  {
    id: "preview-2",
    subject: "Faktura za październik",
    participants: "Biuro Księgowe",
    snippet: "Dziękujemy, dokument został zaksięgowany.",
    date: "wczoraj",
    unread: false,
    starred: false,
    messages: [
      { id: "m4", from: "Biuro Księgowe", mine: false, date: "wczoraj, 15:03",
        body: "Dziękujemy, dokument został zaksięgowany." },
    ],
  },
];
