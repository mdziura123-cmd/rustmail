import { useState } from "react";
import { api, errText } from "./api";
import { SHORTCUT_HELP } from "./shortcuts";
import type { Account, ImapAccount } from "./types";

export type SettingsTarget = "signature" | "senders" | "offline" | "spam" | "import" | "addGmail" | "addImap";

interface Props {
  gmail: Account[];
  imap: ImapAccount[];
  busy: boolean;
  notify: boolean;
  content: boolean;
  onNotify: (on: boolean) => void;
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
] as const;
type Tab = (typeof TABS)[number]["id"];

export function SettingsDialog(p: Props) {
  const [tab, setTab] = useState<Tab>("general");
  const [msg, setMsg] = useState<string | null>(null);
  const [confirmRollback, setConfirmRollback] = useState(false);
  const test = async () => {
    setMsg(null);
    try {
      await api.testNotification();
      setMsg("Wysłano powiadomienie testowe. Jeśli go nie widzisz, sprawdź ustawienia powiadomień systemu.");
    } catch (e) {
      setMsg(errText(e));
    }
  };
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
                <label className="check">
                  <input type="checkbox" checked={p.notify} onChange={(e) => p.onNotify(e.target.checked)} />
                  Powiadomienia pulpitowe o nowych wiadomościach
                </label>
                <label className="check">
                  <input type="checkbox" checked={p.content} disabled={!p.notify} onChange={(e) => p.onContent(e.target.checked)} />
                  Pokazuj w powiadomieniu nadawcę i temat
                </label>
                <p className="hint">
                  Domyślnie powiadomienie mówi tylko, że przyszła nowa poczta. Treść wiadomości z ostrzeżeniem nigdy nie jest pokazywana.
                  Poczta jest sprawdzana co minutę, gdy aplikacja jest uruchomiona; powiadomienia wymagają programu
                  <code> notify-send</code> w systemie. Wiadomość, która dojdzie do już nieprzeczytanego wątku, nie wywoła powiadomienia.
                </p>
                <div className="modal-actions">
                  <button disabled={!p.notify} onClick={() => void test()}>Wyślij próbne powiadomienie</button>
                </div>
              </>
            )}
            {tab === "accounts" && (
              <>
                <p className="hint">Zalogowane konta:</p>
                <ul className="acc-list">
                  {p.gmail.map((a) => <li key={"g" + a.email}>{a.email} <span className="hint">(Gmail)</span></li>)}
                  {p.imap.map((a) => <li key={"i" + a.email}>{a.email} <span className="hint">(WP / IMAP)</span></li>)}
                  {p.gmail.length + p.imap.length === 0 && <li className="hint">Brak zalogowanych kont.</li>}
                </ul>
                <div className="settings-list">
                  <button disabled={p.busy} onClick={() => p.onOpen("addGmail")}>+ Dodaj konto Gmail…</button>
                  <button onClick={() => p.onOpen("addImap")}>+ Dodaj konto WP / inne IMAP…</button>
                </div>
                <p className="hint">Wylogowanie: wybierz konto w przełączniku u góry po lewej i kliknij „Wyloguj”.</p>
              </>
            )}
            {tab === "mail" && (
              <div className="settings-list">
                <button onClick={() => p.onOpen("signature")}>Podpis…</button>
                <button onClick={() => p.onOpen("senders")}>Listy nadawców…</button>
                <button onClick={() => p.onOpen("offline")}>Tryb offline…</button>
                <button onClick={() => p.onOpen("import")}>Import poczty z pliku…</button>
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
                <table className="keys">
                  <tbody>
                    {SHORTCUT_HELP.map((s) => (
                      <tr key={s.keys}><th scope="row"><kbd>{s.keys}</kbd></th><td>{s.what}</td></tr>
                    ))}
                  </tbody>
                </table>
                <p className="hint">Skróty nie działają podczas pisania w polu tekstowym ani w otwartym oknie.</p>
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
