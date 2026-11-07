// Czysta logika odpowiadania i przekazywania (bez React i bez DOM) – łatwa do przetestowania.
import type { MessageView } from "./types";

export type ComposeMode = "new" | "reply" | "replyAll" | "forward" | "draft";

export interface ComposeInit {
  mode: ComposeMode;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  html: string;
  threadId: string | null;
  inReplyTo: string | null;
  references: string | null;
  draftId: string | null;
  note?: string;
}

export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Dzieli listę adresów po przecinkach i średnikach, ale nie wewnątrz cudzysłowów ani nawiasów <>. */
export function splitList(s: string): string[] {
  const out: string[] = [];
  let cur = "";
  let quote = false;
  let angle = false;
  for (const ch of s) {
    if (ch === '"') quote = !quote;
    else if (!quote && ch === "<") angle = true;
    else if (!quote && ch === ">") angle = false;
    if (!quote && !angle && (ch === "," || ch === ";")) {
      if (cur.trim()) out.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

export function emailOf(addr: string): string {
  const l = addr.lastIndexOf("<");
  const r = addr.lastIndexOf(">");
  return (l >= 0 && r > l ? addr.slice(l + 1, r) : addr).trim().toLowerCase();
}

function uniqueAddrs(list: string[], exclude: string[]): string[] {
  const seen = new Set(exclude.map((e) => e.toLowerCase()));
  const out: string[] = [];
  for (const a of list) {
    const e = emailOf(a);
    if (!e || seen.has(e)) continue;
    seen.add(e);
    out.push(a);
  }
  return out;
}

export function subjectWith(prefix: "Re" | "Fwd", subject: string): string {
  const s = subject.trim();
  if (prefix === "Re") return /^(re|odp)\s*:/i.test(s) ? s : `Re: ${s}`;
  return /^(fwd?|prz)\s*:/i.test(s) ? s : `Fwd: ${s}`;
}

/** Wiadomość, na którą odpowiadamy: ostatnia od kogoś innego, a gdy nie ma takiej – ostatnia w ogóle. */
export function replyTarget(messages: MessageView[]): MessageView | null {
  for (let i = messages.length - 1; i >= 0; i--) if (!messages[i].mine) return messages[i];
  return messages.length ? messages[messages.length - 1] : null;
}

export function replyRecipients(m: MessageView, me: string, all: boolean): { to: string; cc: string } {
  const mine = [me];
  if (m.mine) {
    // Odpowiedź na własną wiadomość trafia do tych samych adresatów.
    return {
      to: uniqueAddrs(splitList(m.to), mine).join(", "),
      cc: all ? uniqueAddrs(splitList(m.cc), mine).join(", ") : "",
    };
  }
  const first = m.replyTo.trim() || m.fromEmail;
  if (!all) return { to: uniqueAddrs([first], mine).join(", "), cc: "" };
  const to = uniqueAddrs([first, ...splitList(m.to)], mine);
  const toEmails = to.map(emailOf);
  const cc = uniqueAddrs(splitList(m.cc), [...mine, ...toEmails]);
  return { to: to.join(", "), cc: cc.join(", ") };
}

export function signatureHtml(sig: string): string {
  const t = sig.trim();
  if (!t) return "";
  return `<div>-- <br>${escapeHtml(t).replace(/\r?\n/g, "<br>")}</div>`;
}

function bodyHtml(text: string): string {
  return escapeHtml(text.slice(0, 20000).trimEnd()).replace(/\r?\n/g, "<br>");
}

export function buildReply(
  messages: MessageView[], subject: string, me: string, all: boolean, signature: string,
  threadId: string, fmtDate: (ms: number) => string,
): ComposeInit | null {
  const m = replyTarget(messages);
  if (!m) return null;
  const r = replyRecipients(m, me, all);
  const quote =
    `<div>${escapeHtml(fmtDate(m.dateMs))}, ${escapeHtml(m.from === "Ja" ? me : m.from)} napisał(a):</div>` +
    `<blockquote>${bodyHtml(m.body)}</blockquote>`;
  return {
    mode: all ? "replyAll" : "reply",
    to: r.to,
    cc: r.cc,
    bcc: "",
    subject: subjectWith("Re", subject),
    html: `<div><br></div>${signatureHtml(signature)}<div><br></div>${quote}`,
    threadId,
    inReplyTo: m.messageId || null,
    references: [m.references, m.messageId].filter((x) => x.trim()).join(" ") || null,
    draftId: null,
  };
}

export function buildForward(
  messages: MessageView[], subject: string, me: string, signature: string,
  fmtDate: (ms: number) => string,
): ComposeInit | null {
  const m = messages.length ? messages[messages.length - 1] : null;
  if (!m) return null;
  const head =
    `<div>---------- Przekazana wiadomość ----------</div>` +
    `<div>Od: ${escapeHtml(m.mine ? me : `${m.from} <${m.fromEmail}>`)}</div>` +
    `<div>Data: ${escapeHtml(fmtDate(m.dateMs))}</div>` +
    `<div>Temat: ${escapeHtml(subject)}</div>` +
    `<div>Do: ${escapeHtml(m.to)}</div><div><br></div>`;
  return {
    mode: "forward",
    to: "",
    cc: "",
    bcc: "",
    subject: subjectWith("Fwd", subject),
    html: `<div><br></div>${signatureHtml(signature)}<div><br></div>${head}<blockquote>${bodyHtml(m.body)}</blockquote>`,
    threadId: null,
    inReplyTo: null,
    references: null,
    draftId: null,
    note: m.attachments.length
      ? "Załączniki oryginalnej wiadomości nie są przekazywane – dodaj je ponownie, jeśli są potrzebne."
      : undefined,
  };
}

export function buildNew(signature: string): ComposeInit {
  return {
    mode: "new", to: "", cc: "", bcc: "", subject: "",
    html: `<div><br></div>${signatureHtml(signature)}`,
    threadId: null, inReplyTo: null, references: null, draftId: null,
  };
}
