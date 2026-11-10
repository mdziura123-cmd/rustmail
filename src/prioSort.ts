/** Flagi ważności: 0 = brak, 1 = bardzo ważne, 2 = ważne, 3 = mniej ważne. */
export type PrioLevel = 0 | 1 | 2 | 3;

export const PRIOS: { level: 1 | 2 | 3; label: string; color: string }[] = [
  { level: 1, label: "Bardzo ważne", color: "#d92d20" },
  { level: 2, label: "Ważne", color: "#f79009" },
  { level: 3, label: "Mniej ważne", color: "#2e90fa" },
];

export const gmailPrioKey = (account: string, threadId: string) => `g:${account}:${threadId}`;
export const imapPrioKey = (account: string, folder: string, uid: number) => `i:${account}:${folder}:${uid}`;

/**
 * Kolejność wyników wyszukiwania: najpierw wiadomości z flagą (bardzo ważne, ważne, mniej ważne),
 * a w obrębie jednej flagi i wśród wiadomości bez flagi – od najnowszych (po dacie odebrania).
 */
export function sortSearchResults<T>(items: T[], dateOf: (t: T) => number, levelOf: (t: T) => number): T[] {
  return items
    .map((t, i) => ({ t, i, l: levelOf(t), d: dateOf(t) }))
    .sort((a, b) => {
      const fa = a.l > 0;
      const fb = b.l > 0;
      if (fa !== fb) return fa ? -1 : 1;
      if (fa && a.l !== b.l) return a.l - b.l;
      if (a.d !== b.d) return b.d - a.d;
      return a.i - b.i;
    })
    .map((x) => x.t);
}
