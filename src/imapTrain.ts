// Uczenie lokalnego filtra spamu decyzjami użytkownika w kontach IMAP (tak jak w Gmailu).
import { api } from "./api";

/** Identyfikator wiadomości dla modelu: tylko litery i cyfry, do 64 znaków, numer UID na początku. */
export function imapTrainId(account: string, folder: string, uid: number): string {
  return ("i" + uid + (account + folder).replace(/[^a-zA-Z0-9]/g, "")).slice(0, 64);
}

/** Uczy filtr: `spam` = oznaczono jako spam, inaczej = „to nie spam”. Błędy są pomijane. */
export function trainImap(account: string, folder: string, items: { uid: number; subject: string; from: string; text?: string }[], spam: boolean) {
  const list = items.slice(0, 200).map((m) => ({
    id: imapTrainId(account, folder, m.uid), subject: m.subject, snippet: (m.text ?? "").slice(0, 400), participants: m.from,
  }));
  if (list.length > 0) api.spamTrain(list, spam).catch(() => undefined);
}
