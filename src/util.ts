export function fmtDate(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString("pl-PL", { hour: "2-digit", minute: "2-digit" });
  }
  return d.toLocaleDateString("pl-PL", {
    day: "2-digit",
    month: "2-digit",
    year: d.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}

export function fmtFull(ms: number): string {
  if (!ms) return "";
  return new Date(ms).toLocaleString("pl-PL", {
    day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
  });
}

export function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
