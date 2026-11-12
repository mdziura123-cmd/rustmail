// Polskie święta państwowe i kościelne wolne od pracy (obliczane lokalnie, bez internetu).
export interface Holiday { name: string; desc: string }

const pad = (n: number) => String(n).padStart(2, "0");
export const dayKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** Niedziela Wielkanocna (algorytm Meeusa/Jonesa/Butchera, kalendarz gregoriański). */
export function easterSunday(y: number): Date {
  const a = y % 19, b = Math.floor(y / 100), c = y % 100;
  const d = Math.floor(b / 4), e = b % 4, f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4), k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7;
  const m = Math.floor((a + 11 * h + 22 * l) / 451);
  const month = Math.floor((h + l - 7 * m + 114) / 31);
  const day = ((h + l - 7 * m + 114) % 31) + 1;
  return new Date(y, month - 1, day);
}

const cache = new Map<number, Map<string, Holiday>>();

/** Dni wolne od pracy w danym roku (klucz „RRRR-MM-DD”). */
export function holidaysOf(y: number): Map<string, Holiday> {
  const hit = cache.get(y);
  if (hit) return hit;
  const m = new Map<string, Holiday>();
  const add = (d: Date, name: string, desc: string) => m.set(dayKey(d), { name, desc });
  const fixed = (mo: number, d: number, name: string, desc: string) => add(new Date(y, mo - 1, d), name, desc);
  fixed(1, 1, "Nowy Rok", "Pierwszy dzień nowego roku, a zarazem Uroczystość Świętej Bożej Rodzicielki.");
  fixed(1, 6, "Trzech Króli", "Objawienie Pańskie: pamiątka hołdu Mędrców ze Wschodu złożonego Dzieciątku Jezus. Dzień wolny od 2011 r.");
  const easter = easterSunday(y);
  const off = (n: number) => new Date(y, easter.getMonth(), easter.getDate() + n);
  add(easter, "Wielkanoc", "Niedziela Zmartwychwstania Pańskiego, najważniejsze święto chrześcijańskie. Przypada w pierwszą niedzielę po pierwszej wiosennej pełni Księżyca.");
  add(off(1), "Poniedziałek Wielkanocny", "Drugi dzień Wielkanocy; tradycyjnie śmigus-dyngus, czyli oblewanie się wodą.");
  fixed(5, 1, "Święto Pracy", "Międzynarodowe Święto Pracy.");
  fixed(5, 3, "Święto Konstytucji 3 Maja", "Rocznica uchwalenia w 1791 r. Konstytucji 3 maja, pierwszej w Europie nowoczesnej konstytucji.");
  add(off(49), "Zielone Świątki", "Niedziela Zesłania Ducha Świętego (Pięćdziesiątnica), 50 dni po Wielkanocy.");
  add(off(60), "Boże Ciało", "Uroczystość Najświętszego Ciała i Krwi Chrystusa, w czwartek po Zielonych Świątkach; tradycyjne procesje.");
  fixed(8, 15, "Wniebowzięcie NMP", "Wniebowzięcie Najświętszej Maryi Panny, a zarazem Święto Wojska Polskiego (rocznica Bitwy Warszawskiej 1920).");
  fixed(11, 1, "Wszystkich Świętych", "Uroczystość ku czci wszystkich świętych; Polacy odwiedzają groby bliskich i zapalają znicze.");
  fixed(11, 11, "Narodowe Święto Niepodległości", "Rocznica odzyskania niepodległości w 1918 r.");
  if (y >= 2025) fixed(12, 24, "Wigilia Bożego Narodzenia", "Wieczerza wigilijna; dzień wolny od pracy od 2025 r.");
  fixed(12, 25, "Boże Narodzenie", "Uroczystość Narodzenia Pańskiego, pierwszy dzień świąt.");
  fixed(12, 26, "Drugi dzień Bożego Narodzenia", "Święto Świętego Szczepana, drugi dzień świąt.");
  cache.set(y, m);
  return m;
}

export function holidayOn(d: Date): Holiday | null {
  return holidaysOf(d.getFullYear()).get(dayKey(d)) ?? null;
}

/** Sobota lub niedziela. */
export const isWeekend = (d: Date) => d.getDay() === 0 || d.getDay() === 6;
