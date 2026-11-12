// Emotki bez rysowania kolorowych glifów: w oknie pisania widać opis w nawiasach, np. [uśmiech];
// dopiero przy wysyłaniu opis zamienia się na prawdziwą emotkę (odbiorca zobaczy ją w swoim programie).
// RustMail nie wyświetla kolorowych emotek, bo zawieszały WebKit na niektórych systemach (zob. util.stripEmoji).

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

const BY_NAME = new Map(EMOJI.map((e) => [e.name, e.char]));

export const token = (name: string) => `[${name}]`;

/** Zamienia opisy [nazwa] na prawdziwe emotki (tylko znane nazwy). Wywoływać wyłącznie przy wysyłaniu. */
export function expandEmoji(s: string): string {
  return s.replace(/\[([^[\]\n<>]{2,24})\]/g, (m, name: string) => BY_NAME.get(name) ?? m);
}
