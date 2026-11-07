// Oddziela cytowaną część odpowiedzi (np. "> Cześć" i wiersz „... napisał:”) od nowej treści wiadomości.
// Czysta logika: nigdy nie ukrywa całej wiadomości – jeśli po odcięciu nic by nie zostało, zwraca tekst w całości.

const ATTRIBUTION = /(napisał|napisała|napisał\(a\)|wrote|schrieb|a écrit|escribió)(?![\p{L}])/iu;
const OUTLOOK = /^\s*(-{2,}\s*(original message|oryginalna wiadomość|wiadomość oryginalna|przekazana wiadomość|forwarded message)\s*-{2,}|_{10,})\s*$/i;

export interface Split {
  main: string;
  quoted: string;
}

export function splitQuoted(text: string): Split {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let cut = -1;

  // 1) Blok cytatu "> ..." ciągnący się do końca wiadomości
  for (let i = 0; i < lines.length; i++) {
    if (!/^\s*>/.test(lines[i])) continue;
    const tailQuoted = lines.slice(i).every((l) => l.trim() === "" || /^\s*>/.test(l));
    if (tailQuoted) { cut = i; break; }
  }
  // dołącz wiersz(e) z informacją „kto napisał”, tuż przed cytatem
  if (cut > 0) {
    let j = cut - 1;
    while (j >= 0 && lines[j].trim() === "") j--;
    if (j >= 0 && /:\s*$/.test(lines[j]) && lines[j].length < 300) {
      let start = j;
      // atrybucja zawinięta na dwie linie
      if (!ATTRIBUTION.test(lines[j]) && j > 0 && ATTRIBUTION.test(lines[j - 1]) && lines[j - 1].trim() !== "") start = j - 1;
      // sama końcówka „wrote:” w osobnej linii – atrybucja zaczyna się linię wyżej
      if (start === j && ATTRIBUTION.test(lines[j]) && lines[j].trim().length < 40 && j > 0 && lines[j - 1].trim() !== "" && !/^\s*>/.test(lines[j - 1])) start = j - 1;
      if (ATTRIBUTION.test(lines[start]) || ATTRIBUTION.test(lines[j]) || start !== j) cut = start;
    }
  }

  // 2) Separator w stylu Outlooka
  if (cut < 0) {
    const o = lines.findIndex((l) => OUTLOOK.test(l));
    if (o >= 0) cut = o;
  }

  if (cut < 0) return { main: text, quoted: "" };
  const main = lines.slice(0, cut).join("\n").trimEnd();
  if (!main.trim()) return { main: text, quoted: "" };
  return { main, quoted: lines.slice(cut).join("\n").trim() };
}
