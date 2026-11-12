import { useEffect, useState, useSyncExternalStore } from "react";
import { playSound, SOUNDS, type SoundId } from "./sound";
import { api, errText } from "./api";
import { AddressBook } from "./AddressBook";
import { Templates } from "./Templates";
import { Backup } from "./Backup";
import { Rules } from "./Rules";
import { clearLog, useLog } from "./eventLog";
import { getLabels, getScale, getUndo, setLabels, setScale, setUndo, SCALES, UNDO_DELAYS } from "./display";
import { BINDINGS, customKeys, keyLabel, keyOf, keysVersion, resetKeys, setKey, subscribeKeys } from "./shortcuts";
import { isThemeId, THEMES, type ThemeId } from "./themes";
import { getProvider, PROVIDERS, setProvider, type Provider } from "./translateProviders";
import type { Account, ImapAccount } from "./types";

export type SettingsTarget = "signature" | "senders" | "offline" | "spam" | "import" | "addGmail" | "addImap";

interface Props {
  gmail: Account[];
  imap: ImapAccount[];
  busy: boolean;
  activeOnly: boolean;
  onActiveOnly: (on: boolean) => void;
  onSignOut: () => void;
  onSignOutImap: (email: string) => void;
  notify: boolean;
  content: boolean;
  background: boolean;
  onBackground: (on: boolean) => void;
  onNotify: (on: boolean) => void;
  theme: ThemeId;
  onTheme: (t: ThemeId) => void;
  onKeysChanged: () => void;
  sound: SoundId;
  onSound: (s: SoundId) => void;
  onContent: (on: boolean) => void;
  autoUpdate: boolean;
  onAutoUpdate: (on: boolean) => void;
  hasPrevious: boolean;
  onRollback: () => void;
  onCheckUpdate: () => void;
  onOpen: (what: SettingsTarget) => void;
  onClose: () => void;
}

const TABS = [
  { id: "general", label: "Ogólne" },
  { id: "accounts", label: "Konta" },
  { id: "mail", label: "Poczta" },
  { id: "spam", label: "Filtr spamu" },
  { id: "shortcuts", label: "Skróty" },
  { id: "updates", label: "Aktualizacje" },
  { id: "log", label: "Dziennik" },
] as const;
type Tab = (typeof TABS)[number]["id"];

/** Dziennik zdarzeń z bieżącej sesji (tylko w pamięci) – do diagnozy, gdy coś nie działa. */
function LogView() {
  const log = useLog();
  const text = log.map((e) => `${new Date(e.t).toLocaleTimeString("pl-PL")} [${e.kind}] ${e.text}`).join("\n");
  const [note, setNote] = useState<string | null>(null);
  return (
    <>
      <p className="hint">Ostatnie zdarzenia z tej sesji (do 200). Dziennik jest tylko w pamięci i znika po zamknięciu programu; nie zawiera treści wiadomości.</p>
      <div className="modal-actions">
        <button disabled={log.length === 0} onClick={() => { void navigator.clipboard.writeText(text).then(() => setNote("Skopiowano.")).catch(() => setNote("Nie udało się skopiować.")); }}>Kopiuj</button>
        <button disabled={log.length === 0} onClick={clearLog}>Wyczyść</button>
      </div>
      {note && <p className="hint" role="status">{note}</p>}
      <pre className="log-box" aria-label="Dziennik zdarzeń">{text || "Brak zdarzeń."}</pre>
    </>
  );
}

/** Wybór usługi tłumaczenia i jej klucza API. Klucze trafiają do systemowego magazynu haseł, nie do plików aplikacji. */
function TranslateSettings() {
  const [provider, setProv] = useState<Provider>(getProvider);
  const [keys, setKeys] = useState<{ deepl: boolean; google: boolean; microsoft: boolean }>({ deepl: false, google: false, microsoft: false });
  const [key, setKey] = useState("");
  const [region, setRegion] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [consent, setConsent] = useState<{ enabled: boolean; granted: string[] } | null>(null);
  const refresh = () => {
    api.translateKeys().then(setKeys).catch(() => {});
    api.translateConsent().then(setConsent).catch(() => {});
  };
  useEffect(refresh, []);
  const info = PROVIDERS.find((p) => p.id === provider) ?? PROVIDERS[0];
  const has = provider !== "mymemory" && keys[provider];
  const choose = (p: Provider) => { setProv(p); setProvider(p); setKey(""); setNote(null); };
  const save = async () => {
    setNote(null);
    try {
      await api.translateSetKey(provider, key, provider === "microsoft" ? region : undefined);
      setKey("");
      setNote("Klucz zapisany.");
      refresh();
    } catch (e) { setNote(errText(e)); }
  };
  const clear = async () => {
    setNote(null);
    try { await api.translateClearKey(provider); setNote("Klucz usunięty."); refresh(); }
    catch (e) { setNote(errText(e)); }
  };
  return (
    <>
      <h4>Tłumacz (angielski → polski)</h4>
      <label className="spam-opt">
        <input type="checkbox" checked={consent?.enabled ?? true}
          onChange={(e) => { api.translateSetEnabled(e.target.checked).then(setConsent).catch((er) => setNote(errText(er))); }} />
        <span>
          <strong>Tłumaczenie online</strong><br />
          <span className="hint">Po wyłączeniu przycisk „Przetłumacz na polski” znika, a aplikacja nie wyśle do żadnej usługi ani jednego znaku treści wiadomości.</span>
        </span>
      </label>
      <p className="hint">Wybierz usługę, której użyje przycisk „Przetłumacz na polski”. Treść wiadomości jest wysyłana do wybranej usługi wyłącznie po kliknięciu i po Twojej zgodzie na tę usługę. Nic nie jest tłumaczone w tle.</p>
      {PROVIDERS.map((p) => (
        <label key={p.id} className="spam-opt">
          <input type="radio" name="tr-provider" checked={provider === p.id} onChange={() => choose(p.id)} />
          <span>
            <strong>{p.name}</strong>
            {p.needsKey && <span className="hint"> {keys[p.id as "deepl" | "google" | "microsoft"] ? "· klucz zapisany ✓" : "· brak klucza"}</span>}
            <br /><span className="hint">{p.hint}</span>
          </span>
        </label>
      ))}
      {info.needsKey && (
        <>
          <div className="modal-actions wrap">
            <input type="password" autoComplete="off" spellCheck={false} placeholder={info.keyHint} value={key}
              aria-label={info.keyHint} onChange={(e) => setKey(e.target.value)} />
            {provider === "microsoft" && (
              <input autoComplete="off" spellCheck={false} placeholder="Region (np. westeurope, domyślnie global)" value={region}
                aria-label="Region zasobu Azure" onChange={(e) => setRegion(e.target.value)} />
            )}
            <button disabled={!key.trim()} onClick={() => void save()}>{has ? "Zmień klucz" : "Zapisz klucz"}</button>
            {has && <button onClick={() => void clear()}>Usuń klucz</button>}
          </div>
          <p className="hint">Klucz jest przechowywany w systemowym magazynie haseł i wysyłany wyłącznie do tej usługi.</p>
        </>
      )}
      <div className="modal-actions wrap">
        <button disabled={!consent || consent.granted.length === 0}
          onClick={() => { api.translateRevoke().then((c) => { setConsent(c); setNote("Zgody cofnięte – przed następnym tłumaczeniem aplikacja zapyta ponownie."); }).catch((er) => setNote(errText(er))); }}>
          Cofnij zgody na wysyłanie treści{consent && consent.granted.length > 0 ? ` (${consent.granted.length})` : ""}
        </button>
      </div>
      {note && <p className="hint" role="status">{note}</p>}
    </>
  );
}

/** Edytor skrótów: „Zmień”, potem naciśnięcie nowego klawisza. Zmiany zapisują się od razu. */
function KeyEditor({ onChanged }: { onChanged: () => void }) {
  useSyncExternalStore(subscribeKeys, keysVersion);
  const [capturing, setCapturing] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    if (!capturing) return;
    const h = (e: KeyboardEvent) => {
      if (["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock"].includes(e.key)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") { setCapturing(null); setErr(null); return; }
      if (e.ctrlKey || e.altKey || e.metaKey) { setErr("Skrót nie może używać Ctrl, Alt ani Meta."); return; }
      const b = BINDINGS.find((x) => x.cmd === capturing);
      if (!b) return;
      const r = setKey(b.cmd, e.key);
      if (r) { setErr(r); return; }
      setErr(null); setCapturing(null); onChanged();
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  }, [capturing, onChanged]);
  const changed = Object.keys(customKeys()).length > 0;
  return (
    <>
      <table className="keys">
        <tbody>
          {BINDINGS.map((b) => (
            <tr key={b.cmd}>
              <th scope="row"><kbd>{capturing === b.cmd ? "naciśnij klawisz…" : keyLabel(keyOf(b.cmd))}</kbd></th>
              <td>{b.what}</td>
              <td>
                <button className="small" onClick={() => { setErr(null); setCapturing(capturing === b.cmd ? null : b.cmd); }}>
                  {capturing === b.cmd ? "Anuluj" : "Zmień"}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {err && <p className="error" role="alert">{err}</p>}
      <p className="hint">
        Kliknij „Zmień” i naciśnij nowy klawisz (z Shiftem dla wielkiej litery). Klawisz musi być wolny. Stałe są: Delete (usuń),
        Esc i ? (pomoc), Ctrl+A oraz klik z Ctrl lub Shift. Skróty nie działają podczas pisania w polu tekstowym ani w otwartym oknie.
      </p>
      <div className="modal-actions">
        <button disabled={!changed} onClick={() => { resetKeys(); setCapturing(null); setErr(null); onChanged(); }}>Przywróć domyślne</button>
      </div>
    </>
  );
}

export function SettingsDialog(p: Props) {
  const [tab, setTab] = useState<Tab>("general");
  const [msg, setMsg] = useState<string | null>(null);
  const [confirmRollback, setConfirmRollback] = useState(false);
  const [confirmOut, setConfirmOut] = useState<string | null>(null);
  const [scale, setScaleState] = useState(getScale);
  const [undo, setUndoState] = useState(getUndo);
  const [labels, setLabelsState] = useState(getLabels);
  const [trayOk, setTrayOk] = useState<boolean | null>(null);
  const [closeMode, setCloseMode] = useState<string>(() => { try { return localStorage.getItem("rm.closemode") === "taskbar" ? "taskbar" : "tray"; } catch { return "tray"; } });
  const pickCloseMode = (m: string) => {
    setCloseMode(m);
    try { localStorage.setItem("rm.closemode", m); } catch { /* pamięć niedostępna */ }
    void api.setCloseMinimize(m === "taskbar").catch(() => undefined);
  };
  useEffect(() => { void api.backgroundState().then(setTrayOk).catch(() => setTrayOk(null)); }, []);
  const test = async () => {
    setMsg(null);
    try {
      await api.testNotification();
      setMsg("Wysłano powiadomienie testowe. Jeśli go nie widzisz, sprawdź ustawienia powiadomień systemu.");
    } catch (e) {
      setMsg(errText(e));
    }
  };
  const out = (key: string, run: () => void) =>
    confirmOut === key ? (
      <>
        <button className="small warn" onClick={() => { setConfirmOut(null); run(); }}>Na pewno? Kliknij ponownie</button>
        <button className="small" onClick={() => setConfirmOut(null)}>Anuluj</button>
      </>
    ) : (
      <button className="small" onClick={() => setConfirmOut(key)}>Wyloguj</button>
    );
  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Ustawienia">
      <div className="modal settings-modal">
        <h3>Ustawienia</h3>
        <div className="settings-cols">
          <nav className="settings-tabs" aria-label="Kategorie ustawień">
            {TABS.map((t) => (
              <button key={t.id} className={"linklike plain" + (t.id === tab ? " active" : "")}
                aria-current={t.id === tab} onClick={() => { setTab(t.id); setMsg(null); }}>{t.label}</button>
            ))}
          </nav>
          <div className="settings-body">
            {tab === "general" && (
              <>
                <div className="check sound-row">
                  <label htmlFor="theme-pick">Motyw</label>
                  <select id="theme-pick" value={p.theme} onChange={(e) => p.onTheme(isThemeId(e.target.value) ? e.target.value : "auto")}>
                    {THEMES.map((t) => <option key={t.id} value={t.id}>{t.label}</option>)}
                  </select>
                </div>
                <div className="check sound-row">
                  <label htmlFor="scale-pick">Skala interfejsu</label>
                  <select id="scale-pick" value={scale} onChange={(e) => { const n = Number(e.target.value); setScaleState(n); setScale(n); }}>
                    {SCALES.map((s) => <option key={s} value={s}>{Math.round(s * 100)}%</option>)}
                  </select>
                </div>
                <div className="check sound-row">
                  <label htmlFor="undo-pick">Cofnięcie wysyłki (czas na rozmyślenie)</label>
                  <select id="undo-pick" value={undo} onChange={(e) => { const n = Number(e.target.value); setUndoState(n); setUndo(n); }}>
                    {UNDO_DELAYS.map((s) => <option key={s} value={s}>{s === 0 ? "wyłączone" : s + " s"}</option>)}
                  </select>
                </div>
                <label className="check">
                  <input type="checkbox" checked={labels} onChange={(e) => { setLabelsState(e.target.checked); setLabels(e.target.checked); }} />
                  Podpisy pod ikonami (wygodne na ekranie dotykowym)
                </label>
                <label className="check">
                  <input type="checkbox" checked={p.notify} onChange={(e) => p.onNotify(e.target.checked)} />
                  Powiadomienia pulpitowe o nowych wiadomościach
                </label>
                <label className="check">
                  <input type="checkbox" checked={p.content} disabled={!p.notify} onChange={(e) => p.onContent(e.target.checked)} />
                  Pokazuj w powiadomieniu nadawcę i temat
                </label>
                <label className="check">
                  <input type="checkbox" checked={p.activeOnly} onChange={(e) => p.onActiveOnly(e.target.checked)} />
                  Sprawdzaj pocztę w tle tylko na koncie, na którym pracuję
                </label>
                <label className="check">
                  <input type="checkbox" checked={p.background} onChange={(e) => p.onBackground(e.target.checked)} />
                  Po zamknięciu okna zostaw RustMail w tle (ikona w zasobniku)
                </label>
                {p.background && (
                  <div className="check sound-row">
                    <label htmlFor="close-mode">Po zamknięciu okna</label>
                    <select id="close-mode" value={closeMode} onChange={(e) => pickCloseMode(e.target.value)}>
                      <option value="tray">schowaj do zasobnika</option>
                      <option value="taskbar">zminimalizuj do paska zadań</option>
                    </select>
                  </div>
                )}
                {p.background && closeMode === "tray" && (
                  <p className="hint">Ikonę w zasobniku otwiera menu: Pokaż RustMail. W systemie Linux zasobnik nie przekazuje programowi zwykłego kliknięcia, więc nie da się go otwierać jednym kliknięciem. Przypięta ikona RustMail w pasku zadań otwiera okno jednym kliknięciem, także gdy jest schowane. Możesz też wybrać zminimalizowanie do paska zadań.</p>
                )}
                {p.background && trayOk === false && closeMode === "tray" && (
                  <p className="hint">Ikona w zasobniku nie jest dostępna w tym środowisku, więc zamknięcie okna zamknie program.</p>
                )}
                <div className="check sound-row">
                  <label htmlFor="sound-pick">Dźwięk nowej wiadomości</label>
                  <select id="sound-pick" value={p.sound} onChange={(e) => { const s = e.target.value as SoundId; p.onSound(s); playSound(s); }}>
                    {SOUNDS.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
                  </select>
                  <button type="button" disabled={p.sound === "off"} onClick={() => playSound(p.sound)}>Odtwórz</button>
                </div>
                <p className="hint">
                  Powiadomienie pokazuje nadawcę i temat (bez tej opcji tylko informację, że przyszła nowa poczta). Treść wiadomości z ostrzeżeniem nigdy nie jest pokazywana.
                  Poczta jest sprawdzana co minutę, także gdy aplikacja działa w tle (w zasobniku, po zamknięciu okna). Aby zakończyć program całkiem, wybierz „Zakończ RustMail” z menu ikony w zasobniku.
                  Powiadomienia wymagają programu
                  <code> notify-send</code> w systemie. Wiadomość, która dojdzie do już nieprzeczytanego wątku, nie wywoła powiadomienia.
                </p>
                <div className="modal-actions">
                  <button disabled={!p.notify} onClick={() => void test()}>Wyślij próbne powiadomienie</button>
                </div>
                <TranslateSettings />
              </>
            )}
            {tab === "accounts" && (
              <>
                <p className="hint">Zalogowane konta:</p>
                <ul className="acc-list">
                  {p.gmail.map((a) => (
                    <li key={"g" + a.email}>
                      {a.email} <span className="hint">(Gmail)</span>{" "}
                      {a.active ? out("g" + a.email, p.onSignOut) : <span className="hint">– aby wylogować, przełącz się na to konto</span>}
                    </li>
                  ))}
                  {p.imap.map((a) => (
                    <li key={"i" + a.email}>
                      {a.email} <span className="hint">(WP / IMAP)</span>{" "}
                      {out("i" + a.email, () => p.onSignOutImap(a.email))}
                    </li>
                  ))}
                  {p.gmail.length + p.imap.length === 0 && <li className="hint">Brak zalogowanych kont.</li>}
                </ul>
                <div className="settings-list">
                  <button disabled={p.busy} onClick={() => p.onOpen("addGmail")}>+ Dodaj konto Gmail…</button>
                  <button onClick={() => p.onOpen("addImap")}>+ Dodaj konto WP / inne IMAP…</button>
                </div>
                <p className="hint">Wylogowanie usuwa z tego komputera zapisane dane logowania do konta. Wiadomości na serwerze zostają.</p>
              </>
            )}
            {tab === "mail" && (
              <div className="settings-list">
                <button onClick={() => p.onOpen("signature")}>Podpis…</button>
                <button onClick={() => p.onOpen("senders")}>Listy nadawców…</button>
                <button onClick={() => p.onOpen("offline")}>Tryb offline…</button>
                <button onClick={() => p.onOpen("import")}>Import poczty z pliku…</button>
                <AddressBook />
                <Rules />
                <Templates />
                <Backup />
              </div>
            )}
            {tab === "spam" && (
              <>
                <p className="hint">Automatyczne wykrywanie spamu: tryb działania, silniki, listy złośliwych domen i historia decyzji filtra.</p>
                <div className="modal-actions"><button className="primary" onClick={() => p.onOpen("spam")}>Otwórz ustawienia filtra spamu…</button></div>
              </>
            )}
            {tab === "shortcuts" && (
              <>
                <KeyEditor onChanged={p.onKeysChanged} />
              </>
            )}
            {tab === "updates" && (
              <>
                <label className="check">
                  <input type="checkbox" checked={p.autoUpdate} onChange={(e) => p.onAutoUpdate(e.target.checked)} />
                  Sprawdzaj aktualizacje automatycznie
                </label>
                <div className="modal-actions">
                  <button onClick={p.onCheckUpdate}>Sprawdź aktualizację teraz</button>
                  {p.hasPrevious && (confirmRollback ? (
                    <button className="warn" onClick={() => { setConfirmRollback(false); p.onRollback(); }}>Na pewno? Kliknij ponownie</button>
                  ) : (
                    <button onClick={() => setConfirmRollback(true)}>Przywróć poprzednią wersję</button>
                  ))}
                </div>
              </>
            )}
            {tab === "log" && <LogView />}
            {msg && <p className="hint" role="status">{msg}</p>}
          </div>
        </div>
        <div className="modal-actions">
          <button className="primary" onClick={p.onClose}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
