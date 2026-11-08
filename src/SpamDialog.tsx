import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { SpamConfig, SpamLogEntry, SpamMode, SpamStatus, TrainItem } from "./types";
import { fmtFull } from "./util";

interface Props {
  onClose: () => void;
  /** Po zmianie położenia wiadomości (przeniesienie, przywrócenie) – odśwież listę. */
  onChanged: () => void;
  onToast: (text: string) => void;
}

const MODES: { id: SpamMode; label: string; hint: string }[] = [
  { id: "off", label: "Wyłączony", hint: "Filtr nic nie robi." },
  { id: "flag", label: "Tylko oznaczaj", hint: "Podejrzane wiadomości dostają znacznik „Możliwy spam”, a Ty decydujesz. Nic nie jest przenoszone." },
  { id: "certain", label: "Przenoś tylko pewne (zalecane)", hint: "Do Spamu trafia wiadomość, którą uznają co najmniej dwa silniki albo która ma link lub nadawcę z listy złośliwych domen. Pozostałe podejrzane są tylko oznaczane." },
  { id: "aggressive", label: "Przenoś wszystko, co podejrzane", hint: "Wystarczy głos jednego silnika. Najmocniej czyści skrzynkę, ale część prawdziwych wiadomości może trafić do Spamu." },
];

const ENGINES: { key: "headers" | "rules" | "bayes" | "lists"; label: string; hint: string }[] = [
  { key: "headers", label: "Ocena nagłówków", hint: "SPF, DKIM, DMARC, podszywanie się pod marki i Twoja lista zablokowanych nadawców." },
  { key: "rules", label: "Reguły i słowa kluczowe", hint: "Frazy typowe dla spamu (np. „wygrałeś”, „viagra”) oraz Twoje własne." },
  { key: "bayes", label: "Filtr uczący się (Bayes)", hint: "Uczy się z Twojego Spamu i z tego, co oznaczasz jako spam lub nie spam. Działa dopiero po nauczeniu na co najmniej 20 wiadomościach każdego rodzaju." },
  { key: "lists", label: "Otwarte listy złośliwych domen", hint: "Sprawdza nadawcę i linki w wiadomości z publiczną listą znanych domen z malware i phishingiem. Lista jest pobierana z internetu i odświeżana raz na dobę." },
];

const ACTION_TEXT: Record<string, string> = {
  moved: "przeniesiono do Spamu",
  flagged: "oznaczono jako możliwy spam",
  restored: "przywrócono do skrzynki",
  spammed: "przeniesiono do Spamu (Twoja decyzja)",
  dismissed: "uznano za bezpieczną",
};

function toItem(e: SpamLogEntry): TrainItem {
  return { id: e.id, subject: e.subject, snippet: e.snippet, participants: e.sender };
}

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function SpamDialog({ onClose, onChanged, onToast }: Props) {
  const [cfg, setCfg] = useState<SpamConfig | null>(null);
  const [custom, setCustom] = useState("");
  const [status, setStatus] = useState<SpamStatus | null>(null);
  const [log, setLog] = useState<SpamLogEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try { setStatus(await api.spamStatus()); } catch { /* ignorujemy */ }
    try { setLog(await api.spamLog()); } catch { /* ignorujemy */ }
  }, []);

  useEffect(() => {
    api.spamGetConfig().then((c) => { setCfg(c); setCustom(c.custom.join("\n")); }).catch((e) => setMsg(errText(e)));
    void refresh();
  }, [refresh]);

  const run = async (name: string, f: () => Promise<string | void>) => {
    setBusy(name);
    setMsg(null);
    try {
      const r = await f();
      if (typeof r === "string") setMsg(r);
    } catch (e) {
      setMsg(errText(e));
    } finally {
      setBusy(null);
      void refresh();
    }
  };

  if (!cfg) {
    return (
      <div className="modal-back" role="dialog" aria-modal="true" aria-label="Filtr spamu">
        <div className="modal"><p className="hint">{msg ?? "Wczytywanie…"}</p>
          <div className="modal-actions"><button className="primary" onClick={onClose}>Zamknij</button></div>
        </div>
      </div>
    );
  }

  const set = <K extends keyof SpamConfig>(k: K, v: SpamConfig[K]) => setCfg({ ...cfg, [k]: v });
  const save = () => run("save", async () => {
    const saved = await api.spamSetConfig({ ...cfg, custom: custom.split("\n").map((l) => l.trim()).filter(Boolean) });
    setCfg(saved);
    setCustom(saved.custom.join("\n"));
    onToast("Ustawienia filtra spamu zapisane.");
    return "Zapisano. Skrzynka zostanie sprawdzona przy najbliższym odświeżeniu (co minutę).";
  });

  const bayesLine = status
    ? status.bayesReady
      ? `Nauczony na ${status.bayesSpam} spamie i ${status.bayesHam} zwykłych wiadomościach.`
      : `Za mało danych (${status.bayesSpam} spam / ${status.bayesHam} zwykłych, potrzeba po 20). Kliknij „Ucz z mojej poczty”.`
    : "";
  const listLine = status
    ? status.listDomains > 0
      ? `Wczytano ${status.listDomains.toLocaleString("pl-PL")} domen${status.listAgeHours != null ? ` (sprzed ${status.listAgeHours} godz.)` : ""}.`
      : "Lista jeszcze nie została pobrana."
    : "";

  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Filtr spamu">
      <div className="modal wide-modal">
        <h3>Filtr spamu</h3>
        <p className="hint">Wszystko działa lokalnie na Twoim komputerze. Wiadomości przeniesione do Spamu można w każdej chwili przywrócić (niżej).</p>

        <fieldset className="spam-group">
          <legend>Tryb działania</legend>
          {MODES.map((m) => (
            <label key={m.id} className="spam-opt">
              <input type="radio" name="spam-mode" checked={cfg.mode === m.id} onChange={() => set("mode", m.id)} />
              <span><strong>{m.label}</strong><br /><span className="hint">{m.hint}</span></span>
            </label>
          ))}
        </fieldset>

        <fieldset className="spam-group">
          <legend>Silniki (możesz włączyć dowolne)</legend>
          {ENGINES.map((e) => (
            <label key={e.key} className="spam-opt">
              <input type="checkbox" checked={cfg[e.key]} onChange={(ev) => set(e.key, ev.target.checked)} />
              <span><strong>{e.label}</strong><br /><span className="hint">{e.hint}</span></span>
            </label>
          ))}
          {cfg.lists && (
            <label className="spam-opt">
              <span />
              <span>
                Dostawca listy:{" "}
                <select value={cfg.provider} onChange={(e) => set("provider", e.target.value)}>
                  {(status?.providers ?? [{ id: cfg.provider, label: cfg.provider }]).map((p) => (
                    <option key={p.id} value={p.id}>{p.label}</option>
                  ))}
                </select>
                <br /><span className="hint">{listLine} Z listy nigdy nie są blokowane największe serwisy (np. Google, Microsoft, GitHub).</span>
              </span>
            </label>
          )}
          {cfg.rules && (
            <label className="spam-opt">
              <span />
              <span>
                Własne frazy (jedna w wierszu):
                <textarea rows={3} value={custom} onChange={(e) => setCustom(e.target.value)}
                  placeholder="np. tania pożyczka" />
              </span>
            </label>
          )}
        </fieldset>

        <div className="modal-actions wrap">
          <button className="primary" disabled={busy !== null} onClick={() => void save()}>
            {busy === "save" ? "Zapisywanie…" : "Zapisz ustawienia"}
          </button>
          <button disabled={busy !== null} onClick={() => void run("boot", async () => {
            const r = await api.spamBootstrap();
            return `Filtr nauczył się na ${r.spam} wiadomościach ze Spamu i ${r.ham} przeczytanych ze skrzynki.`;
          })}>{busy === "boot" ? "Uczenie…" : "Ucz z mojej poczty"}</button>
          <button disabled={busy !== null} onClick={() => void run("lists", async () => {
            const n = await api.spamRefreshLists();
            return `Lista odświeżona: ${n.toLocaleString("pl-PL")} domen.`;
          })}>{busy === "lists" ? "Pobieranie…" : "Odśwież listy"}</button>
          <button disabled={busy !== null || cfg.mode === "off"} onClick={() => void run("scan", async () => {
            const r = await api.spamScanNow();
            onChanged();
            const notes = r.notes.length ? ` ${r.notes.join(" ")}` : "";
            return `Sprawdzono ${r.checked}: przeniesiono ${r.moved}, oznaczono ${r.flagged}.${notes}`;
          })}>{busy === "scan" ? "Sprawdzanie…" : "Przeskanuj skrzynkę teraz"}</button>
        </div>
        {msg && <p className="hint" role="status">{msg}</p>}
        {bayesLine && <p className="hint">Filtr uczący się: {bayesLine}</p>}
        <p className="hint">„Ucz z mojej poczty” bierze do 100 wątków ze Spamu (jako spam) i do 100 przeczytanych wątków bez ostrzeżeń ze skrzynki (jako zwykłe).</p>

        <h4>Ostatnie decyzje filtra</h4>
        {log.length === 0 ? (
          <p className="hint">Na razie nic.</p>
        ) : (
          <ul className="outbox-list">
            {log.slice(0, 30).map((e, i) => (
              <li key={e.id + i}>
                <strong>{e.subject || "(bez tematu)"}</strong>
                <span className="hint"> od: {e.sender} · {fmtFull(e.ts)} · {ACTION_TEXT[e.action] ?? e.action}</span>
                {e.reason && <span className="hint">{e.reason}</span>}
                {e.open && (
                  <span className="outbox-actions">
                    {e.action === "moved" && (
                      <button className="small" disabled={busy !== null} onClick={() => void run("r" + e.id, async () => {
                        await api.spamRestore(toItem(e)); onChanged(); return "Wiadomość wróciła do skrzynki, a filtr nauczył się, że to nie spam.";
                      })}>Przywróć (to nie spam)</button>
                    )}
                    {e.action === "flagged" && (
                      <>
                        <button className="small" disabled={busy !== null} onClick={() => void run("m" + e.id, async () => {
                          await api.spamMove(toItem(e)); onChanged(); return "Wiadomość przeniesiona do Spamu.";
                        })}>Do spamu</button>
                        <button className="small" disabled={busy !== null} onClick={() => void run("d" + e.id, async () => {
                          await api.spamTrain([toItem(e)], false); return "Uznano za bezpieczną.";
                        })}>To nie spam</button>
                      </>
                    )}
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}

        <div className="modal-actions"><button className="primary" onClick={onClose}>Zamknij</button></div>
      </div>
    </div>
  );
}
