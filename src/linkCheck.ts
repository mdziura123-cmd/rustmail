// Analiza linków pod kątem phishingu (czysta logika, bez DOM – łatwa do przetestowania).

const SECOND_LEVEL = new Set(["co", "com", "org", "net", "gov", "edu", "ac"]);

/** Nazwa hosta z adresu http(s); null gdy adres jest nietypowy. Sprawdza też „dane logowania” przed @. */
export function parseUrl(url: string): { host: string; userinfo: boolean; scheme: string } | null {
  const m = /^(https?):\/\/([^/?#]*)/i.exec(url.trim());
  if (!m) return null;
  const authority = m[2];
  const userinfo = authority.includes("@");
  const hostPort = authority.slice(authority.lastIndexOf("@") + 1);
  const host = hostPort.replace(/:\d*$/, "").replace(/^\[|\]$/g, "").toLowerCase();
  if (!host) return null;
  return { host, userinfo, scheme: m[1].toLowerCase() };
}

/** Domena „rejestrowalna” (przybliżenie): ostatnie dwa człony, a przy co.uk itp. – trzy. */
export function registrable(host: string): string {
  const parts = host.split(".").filter(Boolean);
  if (parts.length <= 2) return parts.join(".");
  const tld = parts[parts.length - 1];
  const sld = parts[parts.length - 2];
  if (tld.length === 2 && SECOND_LEVEL.has(sld)) return parts.slice(-3).join(".");
  return parts.slice(-2).join(".");
}

export function isIpHost(host: string): boolean {
  return /^\d{1,3}(\.\d{1,3}){3}$/.test(host) || host.includes(":");
}

/** Domena „widoczna” w tekście linku (np. „https://bank.pl/logowanie” albo „www.bank.pl”). */
export function domainInText(text: string): string | null {
  const m = /(?:https?:\/\/)?((?:[a-z0-9-]+\.)+[a-z]{2,})(?::\d+)?(?:[/?#\s]|$)/i.exec(text.trim());
  return m ? m[1].toLowerCase() : null;
}

export interface LinkAnalysis {
  host: string | null;
  warnings: string[];
}

export function analyzeLink(text: string, href: string): LinkAnalysis {
  const warnings: string[] = [];
  if (/^mailto:/i.test(href.trim())) return { host: null, warnings };
  const p = parseUrl(href);
  if (!p) return { host: null, warnings: ["Nietypowy adres – zachowaj ostrożność."] };
  if (p.userinfo) warnings.push("Adres zawiera dane przed znakiem @ – to częsta sztuczka, która ukrywa prawdziwą stronę.");
  if (isIpHost(p.host)) warnings.push("Link prowadzi do adresu IP zamiast nazwy strony.");
  if (p.host.split(".").some((l) => l.startsWith("xn--"))) {
    warnings.push("Nazwa strony zawiera znaki międzynarodowe (xn--) – może udawać inną stronę.");
  }
  if (p.scheme === "http") warnings.push("Połączenie nie jest szyfrowane (http).");
  const shown = domainInText(text);
  if (shown && registrable(shown) !== registrable(p.host)) {
    warnings.push(`Tekst linku sugeruje „${shown}”, ale link prowadzi do „${p.host}”.`);
  }
  return { host: p.host, warnings };
}

const RISKY = new Set([
  "exe", "msi", "bat", "cmd", "com", "scr", "pif", "vbs", "vbe", "js", "jse", "wsf", "wsh", "ps1", "psm1",
  "jar", "sh", "bash", "run", "bin", "appimage", "desktop", "app", "dmg", "pkg", "deb", "rpm", "apk",
  "lnk", "reg", "hta", "cpl", "dll", "iso", "img", "docm", "xlsm", "pptm", "dotm", "xlam", "py", "pl", "rb",
]);

/** Czy nazwa pliku sugeruje plik wykonywalny/skrypt (sprawdza każdy człon, np. „faktura.pdf.exe”). */
export function riskyFile(name: string): boolean {
  return name.toLowerCase().split(".").slice(1).some((e) => RISKY.has(e.trim()));
}
