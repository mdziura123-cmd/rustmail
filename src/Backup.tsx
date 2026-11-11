import { useState } from "react";
import { api, errText } from "./api";

// Te same klucze co w backup.rs (SETTING_KEYS) – tylko wygląd i zachowanie, nic wrażliwego.
const KEYS = [
  "rm.theme", "rm.notify", "rm.notifycontent", "rm.sound", "rm.activeonly", "rm.background", "rm.flagsfirst",
  "rm.keys", "rm.scale", "rm.labels", "rm.autoupdate", "rm.left", "rm.mid", "rm.spell", "rm.undo", "rm.templates",
];

export function Backup() {
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const exportAll = async () => {
    setBusy(true);
    setNote(null);
    try {
      const settings: Record<string, string> = {};
      for (const k of KEYS) {
        try { const v = localStorage.getItem(k); if (v !== null) settings[k] = v; } catch { /* pomijamy */ }
      }
      const path = await api.backupExport(settings);
      setNote(`Kopia zapisana: ${path}`);
    } catch (e) { setNote(errText(e)); } finally { setBusy(false); }
  };

  const importFile = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setNote(null);
    try {
      const text = await file.text();
      const r = await api.backupImport(text);
      for (const [k, v] of Object.entries(r.settings)) {
        try { localStorage.setItem(k, v); } catch { /* pomijamy */ }
      }
      setNote(`Wczytano: adresy ${r.contacts}, flagi ${r.flags}, reguły ${r.rules}, ustawienia ${Object.keys(r.settings).length}. Za chwilę aplikacja się odświeży.`);
      window.setTimeout(() => window.location.reload(), 1800);
    } catch (e) { setNote(errText(e)); } finally { setBusy(false); }
  };

  return (
    <div className="settings-list">
      <p className="hint">
        Kopia zawiera dane, których nie ma na serwerze poczty: książkę adresów, flagi ważności, reguły automatyczne i ustawienia wyglądu.
        Nie zawiera haseł, kluczy ani treści wiadomości. Wczytanie kopii tylko dopisuje dane, niczego nie usuwa.
      </p>
      <button disabled={busy} onClick={() => void exportAll()}>Zapisz kopię do pliku (folder Pobrane)</button>
      <label className="backup-file">
        <span className="hint">Wczytaj kopię z pliku:</span>
        <input type="file" accept="application/json,.json" disabled={busy}
          onChange={(e) => { void importFile(e.target.files?.[0]); e.target.value = ""; }} />
      </label>
      {note && <p className="hint" role="status">{note}</p>}
    </div>
  );
}
