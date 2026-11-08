// Czytanie plików poczty (.mbox, .eml) po kawałku, bez wczytywania całego pliku do pamięci.
// Wiadomości są zwracane jako „tekst binarny” (1 znak = 1 bajt), dzięki czemu nic się nie psuje
// przy kodowaniach znaków; btoa() zamienia go bezpośrednio na base64.

const CHUNK = 4 * 1024 * 1024;

function toBinary(buf: ArrayBuffer): string {
  const u = new Uint8Array(buf);
  let s = "";
  for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode(...u.subarray(i, i + 0x8000));
  return s;
}

const looksLikeMail = (m: string) => m.length > 20 && /^[\x21-\x39\x3b-\x7e]+:/m.test(m.slice(0, 4000));

/** Dzieli plik .mbox (mboxo/mboxrd) na pojedyncze wiadomości. */
export async function* mboxMessages(file: File): AsyncGenerator<string> {
  let cur: string[] | null = null;
  let prevBlank = true;
  let rest = "";
  for (let off = 0; off < file.size; off += CHUNK) {
    const text = rest + toBinary(await file.slice(off, off + CHUNK).arrayBuffer());
    const lines = text.split("\n");
    rest = lines.pop() ?? "";
    for (const ln of lines) {
      if (prevBlank && /^From \S+ /.test(ln)) {
        if (cur) { const m = cur.join("\n"); if (looksLikeMail(m)) yield m; }
        cur = [];
        prevBlank = false;
        continue;
      }
      if (cur === null) cur = [];
      cur.push(ln.replace(/^>(>*From )/, "$1"));
      prevBlank = ln === "" || ln === "\r";
    }
  }
  if (rest) { if (cur === null) cur = []; cur.push(rest); }
  if (cur) { const m = cur.join("\n"); if (looksLikeMail(m)) yield m; }
}

/** Plik .eml to jedna wiadomość. */
export async function emlMessage(file: File): Promise<string | null> {
  const m = toBinary(await file.arrayBuffer());
  return looksLikeMail(m) ? m : null;
}

export const toBase64 = (binary: string) => btoa(binary);

export function defaultLabel(fileName: string): string {
  const base = fileName.replace(/\.(mbox|eml|mbx)$/i, "").replace(/[\u0000-\u001f]/g, "").trim() || "poczta";
  return `Import/${base}`.slice(0, 100);
}
