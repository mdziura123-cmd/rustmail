// Emotki jako własne obrazki zapakowane w aplikację (emojiData.ts). System nie rysuje kolorowych glifów,
// bo zawieszały WebKit na niektórych systemach (zob. util.stripEmoji), a obrazki są bezpieczne.
// W oknie pisania emotka to <img data-emo="kod">; przy wysyłaniu zamienia się na prawdziwy znak emoji.
// Przychodzące emoji ze znanego zestawu zamieniamy na znacznik ⟦e:kod⟧, który widok zamienia na obrazek;
// pozostałe emoji są nadal usuwane.
import { EMOJI_IMG } from "./emojiData";

const E = (...cp: number[]) => String.fromCodePoint(...cp);

export const EMOJI: { name: string; char: string }[] = [
  { name: "uśmiech", char: E(0x1f642) }, { name: "szeroki uśmiech", char: E(0x1f600) }, { name: "śmiech", char: E(0x1f604) },
  { name: "łzy ze śmiechu", char: E(0x1f602) }, { name: "mrugnięcie", char: E(0x1f609) }, { name: "rumieniec", char: E(0x1f60a) },
  { name: "zakochany", char: E(0x1f60d) }, { name: "buziak", char: E(0x1f618) }, { name: "okulary", char: E(0x1f60e) },
  { name: "język", char: E(0x1f61b) }, { name: "zamyślony", char: E(0x1f914) }, { name: "zdziwienie", char: E(0x1f62e) },
  { name: "zaskoczenie", char: E(0x1f632) }, { name: "smutek", char: E(0x1f641) }, { name: "płacz", char: E(0x1f622) },
  { name: "złość", char: E(0x1f620) }, { name: "zmęczony", char: E(0x1f634) }, { name: "zakłopotany", char: E(0x1f605) },
  { name: "ulga", char: E(0x1f60c) }, { name: "przytulenie", char: E(0x1f917) },
  { name: "kciuk w górę", char: E(0x1f44d) }, { name: "kciuk w dół", char: E(0x1f44e) }, { name: "ok", char: E(0x1f44c) },
  { name: "brawa", char: E(0x1f44f) }, { name: "dziękuję", char: E(0x1f64f) }, { name: "machanie", char: E(0x1f44b) },
  { name: "zwycięstwo", char: E(0x270c, 0xfe0f) }, { name: "siła", char: E(0x1f4aa) }, { name: "uścisk dłoni", char: E(0x1f91d) },
  { name: "serce", char: E(0x2764, 0xfe0f) }, { name: "złamane serce", char: E(0x1f494) }, { name: "gwiazdka", char: E(0x2b50) },
  { name: "ogień", char: E(0x1f525) }, { name: "iskry", char: E(0x2728) }, { name: "impreza", char: E(0x1f389) },
  { name: "prezent", char: E(0x1f381) }, { name: "tort", char: E(0x1f382) }, { name: "kwiaty", char: E(0x1f490) },
  { name: "róża", char: E(0x1f339) }, { name: "słońce", char: E(0x2600, 0xfe0f) }, { name: "chmura", char: E(0x2601, 0xfe0f) },
  { name: "deszcz", char: E(0x1f327, 0xfe0f) }, { name: "tęcza", char: E(0x1f308) }, { name: "kawa", char: E(0x2615) },
  { name: "pizza", char: E(0x1f355) }, { name: "samochód", char: E(0x1f697) }, { name: "dom", char: E(0x1f3e0) },
  { name: "telefon", char: E(0x1f4de) }, { name: "list", char: E(0x2709, 0xfe0f) }, { name: "kalendarz", char: E(0x1f4c5) },
  { name: "zegar", char: E(0x23f0) }, { name: "pilne", char: E(0x2757) }, { name: "pytanie", char: E(0x2753) },
  { name: "wykonano", char: E(0x2705) }, { name: "błąd", char: E(0x274c) }, { name: "uwaga", char: E(0x26a0, 0xfe0f) },
  { name: "pomysł", char: E(0x1f4a1) }, { name: "pin", char: E(0x1f4cd) }, { name: "klucz", char: E(0x1f511) },
];


// ---------- obrazki ----------

/** Klucz obrazka: kod znaku małymi literami szesnastkowo (np. "1f642"). */
const keyOfCp = (cp: number) => cp.toString(16);
export const emojiSrc = (key: string): string | undefined => EMOJI_IMG[key];
export const keyOfChar = (ch: string): string => keyOfCp(ch.codePointAt(0) ?? 0);

const MARK = /⟦e:([0-9a-f]{2,6})⟧/g;
const PICT = /\p{Extended_Pictographic}/gu;

/** Znane emoji w tekście → znaczniki ⟦e:kod⟧ (reszta zostaje do usunięcia przez stripEmoji). */
export function protectEmoji(s: string): string {
  return s.replace(PICT, (ch) => {
    const k = keyOfChar(ch);
    return EMOJI_IMG[k] ? `⟦e:${k}⟧` : ch;
  });
}

/** Usuwa znaczniki (tam, gdzie obrazek nie ma sensu: fragmenty, kalendarz, tłumaczenie). */
export function dropMarkers(s: string): string {
  return s.replace(MARK, "");
}

/** Tekst ze znacznikami → węzły DOM: zwykły tekst i obrazki emotek (tylko z własnego zestawu). */
export function emojiNodes(text: string, doc: Document): Node[] {
  const t = protectEmoji(text);
  const out: Node[] = [];
  let last = 0;
  for (const m of t.matchAll(MARK)) {
    const src = EMOJI_IMG[m[1]];
    const i = m.index ?? 0;
    if (i > last) out.push(doc.createTextNode(t.slice(last, i)));
    if (src) out.push(emojiImg(m[1], doc));
    last = i + m[0].length;
  }
  if (last < t.length) out.push(doc.createTextNode(t.slice(last)));
  return out;
}

export function emojiImg(key: string, doc: Document): HTMLImageElement {
  const img = doc.createElement("img");
  img.setAttribute("src", EMOJI_IMG[key] ?? "");
  img.setAttribute("alt", "");
  img.setAttribute("class", "emo");
  img.setAttribute("data-emo", key);
  img.setAttribute("draggable", "false");
  return img;
}

/** Zamienia obrazki emotek na prawdziwe znaki emoji. Wywoływać przy wysyłaniu i zapisie wersji roboczej. */
export function imgToChars(html: string): string {
  return html.replace(/<img\b[^>]*\bdata-emo="([0-9a-f]{2,6})"[^>]*>/gi, (_m, key: string) => {
    const cp = parseInt(key, 16);
    return EMOJI_IMG[key] && cp > 0 ? String.fromCodePoint(cp) + (cp < 0x1f000 ? "\ufe0f" : "") : "";
  });
}

export const emojiImgHtml = (key: string): string =>
  `<img class="emo" data-emo="${key}" alt="" draggable="false" src="${EMOJI_IMG[key] ?? ""}">`;
