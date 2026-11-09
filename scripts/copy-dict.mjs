// Kopiuje słowniki Hunspell (polski, angielski) z pakietów npm do public/dict, skąd trafiają do aplikacji.
// Brak słownika nie przerywa budowania – sprawdzanie pisowni po prostu będzie wyłączone.
import fs from "node:fs";
import path from "node:path";

const out = path.resolve("public", "dict");
fs.mkdirSync(out, { recursive: true });
for (const [lang, pkg] of [["pl", "dictionary-pl"], ["en", "dictionary-en"]]) {
  for (const ext of ["aff", "dic"]) {
    const src = path.resolve("node_modules", pkg, `index.${ext}`);
    if (fs.existsSync(src)) {
      fs.copyFileSync(src, path.join(out, `${lang}.${ext}`));
      console.log(`słownik ${lang}.${ext}: ${fs.statSync(src).size} B`);
    } else {
      console.warn(`BRAK słownika: ${src}`);
    }
  }
}
