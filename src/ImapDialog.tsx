import { useCallback, useEffect, useState } from "react";
import { api, errText } from "./api";
import { MessageBody } from "./MessageBody";
import type { ImapAccount, ImapFolder, ImapMsg, ImapRow, MessageView } from "./types";
import { fmtDate, fmtFull } from "./util";

interface Props { onClose: () => void }

const emailOf = (from: string) => (/<([^>]+)>/.exec(from)?.[1] ?? from).trim().toLowerCase();

function toView(m: ImapMsg): MessageView {
  return {
    id: String(m.uid), from: m.from, fromEmail: emailOf(m.from), mine: false, dateMs: m.dateMs,
    body: m.text, htmlOnly: !m.text && !!m.html, html: m.html, attachments: [], to: m.to, cc: m.cc,
    replyTo: "", messageId: "", references: "", risk: { level: "none", score: 0, reasons: [] },
  };
}

export function ImapDialog({ onClose }: Props) {
  const [acc, setAcc] = useState<ImapAccount | null | undefined>(undefined);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // formularz logowania
  const [email, setEmail] = useState("");
  const [pass, setPass] = useState("");
  const [host, setHost] = useState("");
  // skrzynka
  const [folders, setFolders] = useState<ImapFolder[]>([]);
  const [folder, setFolder] = useState<string>("");
  const [rows, setRows] = useState<ImapRow[]>([]);
  const [next, setNext] = useState<number | null>(null);
  const [msg, setMsg] = useState<ImapMsg | null>(null);
  const [openUid, setOpenUid] = useState<number | null>(null);

  useEffect(() => { api.imapStatus().then(setAcc).catch((e) => { setErr(errText(e)); setAcc(null); }); }, []);

  const loadFolder = useCallback(async (name: string, offset: number, append: boolean) => {
    setBusy(true); setErr(null);
    try {
      const p = await api.imapList(name, offset);
      setRows((prev) => (append ? [...prev, ...p.rows] : p.rows));
      setNext(p.next);
    } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  }, []);

  useEffect(() => {
    if (!acc) return;
    setBusy(true);
    api.imapFolders().then((f) => {
      setFolders(f);
      const inbox = f.find((x) => x.role === "inbox") ?? f[0];
      if (inbox) { setFolder(inbox.name); void loadFolder(inbox.name, 0, false); } else setBusy(false);
    }).catch((e) => { setErr(errText(e)); setBusy(false); });
  }, [acc, loadFolder]);

  const login = async () => {
    setBusy(true); setErr(null);
    try { setAcc(await api.imapLogin(email, pass, host)); setPass(""); }
    catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  const logout = async () => {
    try { await api.imapLogout(); } catch (e) { setErr(errText(e)); return; }
    setAcc(null); setFolders([]); setRows([]); setMsg(null); setOpenUid(null);
  };
  const pickFolder = (name: string) => { setFolder(name); setMsg(null); setOpenUid(null); void loadFolder(name, 0, false); };
  const openRow = async (r: ImapRow) => {
    setOpenUid(r.uid); setErr(null);
    try {
      const m = await api.imapGet(folder, r.uid);
      setMsg(m);
      setRows((prev) => prev.map((x) => (x.uid === r.uid ? { ...x, unread: false } : x)));
    } catch (e) { setErr(errText(e)); }
  };
  const act = async (action: string) => {
    if (!msg) return;
    setErr(null);
    try {
      await api.imapAct(folder, msg.uid, action);
      if (action === "trash" || action === "spam") {
        setRows((prev) => prev.filter((x) => x.uid !== msg.uid)); setMsg(null); setOpenUid(null);
      } else {
        const patch = { unread: action === "unread" ? true : action === "read" ? false : undefined, starred: action === "star" ? true : action === "unstar" ? false : undefined };
        setMsg({ ...msg, unread: patch.unread ?? msg.unread, starred: patch.starred ?? msg.starred });
        setRows((prev) => prev.map((x) => (x.uid === msg.uid ? { ...x, unread: patch.unread ?? x.unread, starred: patch.starred ?? x.starred } : x)));
      }
    } catch (e) { setErr(errText(e)); }
  };

  return (
    <div className="modal-back" role="dialog" aria-modal="true" aria-label="Poczta IMAP">
      <div className="modal imap-modal">
        <div className="imap-head">
          <h3>Poczta IMAP{acc ? ` – ${acc.email}` : ""}</h3>
          <span className="grow" />
          {acc && <button className="small" onClick={() => void logout()}>Wyloguj</button>}
          <button className="small" onClick={onClose}>Zamknij</button>
        </div>
        {err && <p className="error" role="alert">{err}</p>}
        {acc === undefined && <p className="hint">Wczytywanie…</p>}
        {acc === null && (
          <div className="imap-login">
            <p className="hint">
              Logowanie do poczty przez IMAP (np. WP, O2). Połączenie jest szyfrowane, a hasło trafia do systemowego sejfu (KDE Wallet).
              W WP włącz w ustawieniach poczty dostęp przez IMAP i użyj hasła do aplikacji zewnętrznych, jeśli WP je wymaga.
              Ta część aplikacji służy na razie tylko do czytania i porządkowania poczty (bez wysyłania).
            </p>
            <label>Adres e-mail<input value={email} onChange={(e) => setEmail(e.target.value)} placeholder="ty@wp.pl" autoComplete="off" spellCheck={false} /></label>
            <label>Hasło<input type="password" value={pass} onChange={(e) => setPass(e.target.value)} autoComplete="off" /></label>
            <label>Serwer IMAP (zostaw puste dla wp.pl i o2.pl)<input value={host} onChange={(e) => setHost(e.target.value)} placeholder="imap.wp.pl" spellCheck={false} /></label>
            <button className="solid" disabled={busy || !email.trim() || !pass} onClick={() => void login()}>{busy ? "Łączę…" : "Zaloguj"}</button>
          </div>
        )}
        {acc && (
          <div className="imap-cols">
            <nav className="imap-folders" aria-label="Foldery">
              {folders.map((f) => (
                <button key={f.name} className={"linklike plain" + (f.name === folder ? " active" : "")} onClick={() => pickFolder(f.name)}>{f.label}</button>
              ))}
            </nav>
            <div className="imap-list">
              {busy && rows.length === 0 && <p className="hint">Wczytywanie…</p>}
              {!busy && rows.length === 0 && <p className="hint">Folder jest pusty.</p>}
              <ul>
                {rows.map((r) => (
                  <li key={r.uid}>
                    <button className={"row" + (r.uid === openUid ? " active" : "") + (r.unread ? " unread" : "")} onClick={() => void openRow(r)}>
                      <span className="who">{r.from}</span>
                      <span className="when">{r.starred ? "★ " : ""}{fmtDate(r.dateMs)}</span>
                      <span className="subj">{r.subject || "(bez tematu)"}</span>
                    </button>
                  </li>
                ))}
              </ul>
              {next !== null && <button className="more" disabled={busy} onClick={() => void loadFolder(folder, next, true)}>{busy ? "Wczytywanie…" : "Wczytaj więcej"}</button>}
            </div>
            <div className="imap-reader">
              {!msg && <p className="hint">Wybierz wiadomość z listy.</p>}
              {msg && (
                <>
                  <h4>{msg.subject || "(bez tematu)"}</h4>
                  <p className="hint">Od: {msg.from}<br />Do: {msg.to}{msg.cc ? <><br />Kopia: {msg.cc}</> : null}<br />{fmtFull(msg.dateMs)}</p>
                  <div className="imap-actions">
                    <button className="small" onClick={() => void act(msg.unread ? "read" : "unread")}>{msg.unread ? "Oznacz jako przeczytaną" : "Oznacz jako nieprzeczytaną"}</button>
                    <button className="small" onClick={() => void act(msg.starred ? "unstar" : "star")}>{msg.starred ? "Zdejmij gwiazdkę" : "Gwiazdka"}</button>
                    <button className="small" onClick={() => void act("spam")}>Do spamu</button>
                    <button className="small" onClick={() => void act("trash")}>Do kosza</button>
                  </div>
                  {msg.attachments.length > 0 && (
                    <p className="hint">Załączniki (pobieranie jeszcze niedostępne): {msg.attachments.map((a) => a.name).join(", ")}</p>
                  )}
                  <MessageBody key={msg.uid} m={toView(msg)} strict={false} />
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
