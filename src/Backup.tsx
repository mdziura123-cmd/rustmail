import { useEffect, useState } from "react";
import { api, errText } from "./api";

// Te same klucze co w backup.rs (SETTING_KEYS) – tylko wygląd i zachowanie, nic wrażliwego.
const KEYS = [
  "rm.theme", "rm.notify", "rm.notifycontent", "rm.sound", "rm.activeonly", "rm.background", "rm.flagsfirst",
  "rm.keys", "rm.scale", "rm.labels", "rm.autoupdate", "rm.left", "rm.mid", "rm.spell", "rm.undo", "rm.templates", "rm.closemode",
];

export function Backup() {
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [syncDir, setSyncDir] = useState<string | null>(null);
  const [syncInput, setSyncInput] = useState("");
  const [syncNote, setSyncNote] = useState<string | null>(null);

  useEffect(() => { api.syncGet().then((d) => { setSyncDir(d); setSyncInput(d ?? ""); }).catch(() => undefined); }, []);

  const setDir = async (dir: string) => {
    setSyncNote(null);
    try {
      const d = await api.syncSet(dir);
      setSyncDir(d);
      setSyncInput(d ?? "");
      if (d) await runSync(); else setSyncNote("Synchronizacja wyłączona.");
    } catch (e) { setSyncNote(errText(e)); }
  };
  const runSync = async () => {
    setBusy(true);
    setSyncNote(null);
    try {
      const r = await api.syncRun();
      setSyncNote(`Zsynchronizowano. Z pliku dopisano: adresy ${r.contacts}, flagi ${r.flags}, reguły ${r.rules}, wydarzenia ${r.events}. Plik: ${r.path}`);
    } catch (e) { setSyncNote(errText(e)); } finally { setBusy(false); }
  };

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
      setNote(`Wczytano: adresy ${r.contacts}, flagi ${r.flags}, reguły ${r.rules}, wydarzenia ${r.events}, ustawienia ${Object.keys(r.settings).length}. Za chwilę aplikacja się odświeży.`);
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
      <h4>Synchronizacja między komputerami</h4>
      <p className="hint">
        Wskaż folder, który i tak synchronizujesz między komputerami (np. Nextcloud, Syncthing, Dropbox). RustMail zapisze tam plik
        rustmail-sync.json z książką adresów, flagami, regułami i wydarzeniami kalendarza i będzie go łączyć z danymi na tym komputerze
        (przy uruchomieniu, co 15 minut i na przycisk). Plik NIE jest zaszyfrowany, a nie zawiera haseł ani treści wiadomości.
        Synchronizacja tylko dopisuje: usunięta reguła lub wydarzenie może wrócić z pliku innego komputera.
      </p>
      <div className="modal-actions wrap">
        <input value={syncInput} placeholder="/home/ty/Nextcloud/RustMail" aria-label="Folder synchronizacji" onChange={(e) => setSyncInput(e.target.value)} />
        <button disabled={busy || !syncInput.trim()} onClick={() => void setDir(syncInput)}>{syncDir ? "Zmień folder" : "Włącz synchronizację"}</button>
        {syncDir && <button disabled={busy} onClick={() => void runSync()}>Synchronizuj teraz</button>}
        {syncDir && <button disabled={busy} onClick={() => void setDir("")}>Wyłącz</button>}
      </div>
      {syncNote && <p className="hint" role="status">{syncNote}</p>}
    </div>
  );
}
