import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import { badPass, fmtFpr, getAttachKeyDefault, getSignDefault, needsPass, pgpMsg, setAttachKeyDefault, setSignDefault } from "./pgp";
import type { PgpKey, PgpStatus } from "./types";

const fmtDate = (ms: number) => new Date(ms).toLocaleDateString("pl-PL");

function keyState(k: PgpKey): string {
  if (k.revoked) return "unieważniony";
  if (k.expired) return "wygasł";
  if (k.expiresMs > 0) return `ważny do ${fmtDate(k.expiresMs)}`;
  return "bez daty ważności";
}

/** Zakładka Ustawień: własne klucze, klucze kontaktów, import i eksport. */
export function PgpSettings({ accounts }: { accounts: string[] }) {
  const [status, setStatus] = useState<PgpStatus | null>(null);
  const [keys, setKeys] = useState<PgpKey[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [sign, setSign] = useState(getSignDefault);
  const [attach, setAttach] = useState(getAttachKeyDefault);

  const reload = useCallback(() => {
    api.pgpKeys().then(setKeys).catch((e) => setErr(errText(e)));
  }, []);
  useEffect(() => {
    api.pgpStatus().then((s) => { setStatus(s); if (s.available) reload(); }).catch((e) => setErr(errText(e)));
  }, [reload]);

  if (!status) return <p className="hint">Sprawdzanie GnuPG…</p>;
  if (!status.available) {
    return (
      <>
        <p className="error" role="alert">Szyfrowanie OpenPGP jest niedostępne: {status.error}</p>
        <p className="hint">RustMail korzysta z systemowego programu GnuPG (w Bazzite jest zainstalowany domyślnie). Sprawdź w terminalu polecenie <code>gpg --version</code>.</p>
      </>
    );
  }
  const own = keys.filter((k) => k.secret);
  const others = keys.filter((k) => !k.secret);
  const withoutKey = accounts.filter((a) => !own.some((k) => k.emails.includes(a.toLowerCase()) && !k.revoked && !k.expired));
  const done = (m: string) => { setErr(null); setMsg(m); reload(); };
  const fail = (e: unknown) => { setMsg(null); setErr(pgpMsg(errText(e))); };

  return (
    <>
      <p className="hint">
        OpenPGP szyfruje treść i załączniki tak, że przeczytają je tylko odbiorcy – ani Google, ani WP, ani nikt po drodze. Podpis
        pozwala odbiorcy sprawdzić, że wiadomość jest od Ciebie i nikt jej nie zmienił. Temat i adresy pozostają jawne.
        Klucze leżą w osobnym pęku kluczy RustMail na tym komputerze ({status.version}); program nie pobiera niczego z serwerów kluczy.
      </p>

      <h4>Twoje klucze</h4>
      {own.length === 0 && <p className="hint">Nie masz jeszcze własnego klucza. Utwórz go poniżej albo zaimportuj istniejący (np. z Thunderbirda lub z ~/.gnupg).</p>}
      <ul className="pgp-keys">{own.map((k) => <KeyRow key={k.fingerprint} k={k} onDone={done} onFail={fail} />)}</ul>
      {withoutKey.length > 0 && <Generate accounts={withoutKey} onDone={done} onFail={fail} />}

      <label className="check">
        <input type="checkbox" checked={sign} onChange={(e) => { setSign(e.target.checked); setSignDefault(e.target.checked); }} />
        Domyślnie podpisuj nowe wiadomości (gdy masz klucz dla konta nadawcy)
      </label>
      <label className="check">
        <input type="checkbox" checked={attach} onChange={(e) => { setAttach(e.target.checked); setAttachKeyDefault(e.target.checked); }} />
        Domyślnie dołączaj mój klucz publiczny (odbiorca będzie mógł odpisać zaszyfrowaną wiadomością)
      </label>

      <h4>Klucze innych osób</h4>
      {others.length === 0 && <p className="hint">Brak. Klucz dostaniesz od osoby, do której chcesz pisać: jako załącznik .asc (przycisk „Importuj klucz” przy wiadomości) albo jako tekst do wklejenia poniżej.</p>}
      <ul className="pgp-keys">{others.map((k) => <KeyRow key={k.fingerprint} k={k} onDone={done} onFail={fail} />)}</ul>

      <Import onDone={done} onFail={fail} />

      <div className="modal-actions">
        <button onClick={() => { api.pgpForget().then(() => done("Zapomniano hasła kluczy – przy następnym użyciu program o nie zapyta.")).catch(fail); }}>
          Zapomnij zapamiętane hasła
        </button>
      </div>
      <p className="hint">
        Hasło klucza jest pamiętane przez 10 minut od użycia (najdłużej godzinę) i zapominane po zamknięciu programu.
        Zrób kopię zapasową klucza prywatnego („Kopia klucza prywatnego”) – bez niej po awarii dysku nie odczytasz zaszyfrowanej poczty.
      </p>
      {msg && <p className="hint" role="status">{msg}</p>}
      {err && <p className="error" role="alert">{err}</p>}
    </>
  );
}

function KeyRow({ k, onDone, onFail }: { k: PgpKey; onDone: (m: string) => void; onFail: (e: unknown) => void }) {
  const [confirmDel, setConfirmDel] = useState(false);
  const [backupAsk, setBackupAsk] = useState(false);
  const [pass, setPass] = useState("");
  const usable = !k.revoked && !k.expired;
  const saveSecret = (p: string | null) => {
    api.pgpSaveKey(k.fingerprint, true, p)
      .then((r) => { setBackupAsk(false); setPass(""); onDone(r.path ? `Zapisano kopię klucza prywatnego: ${r.path}. Przechowuj ją w bezpiecznym miejscu (np. na pendrive).` : "Nie zapisano pliku."); })
      .catch((e) => { const m = errText(e); if (needsPass(m) || badPass(m)) { setBackupAsk(true); if (badPass(m)) onFail(e); } else onFail(e); });
  };
  return (
    <li className={usable ? "" : "muted"}>
      <div><strong>{k.uids[0] ?? k.keyId}</strong> {k.uids.length > 1 && <span className="hint">(+{k.uids.length - 1} adres)</span>}</div>
      <div className="hint">
        <code className="fpr" title="Odcisk klucza – porównaj go z właścicielem (np. przez telefon)">{fmtFpr(k.fingerprint)}</code>
        {" · "}{k.algo} · utworzony {fmtDate(k.createdMs)} · {keyState(k)}
        {!k.canEncrypt && usable && " · nie nadaje się do szyfrowania"}
      </div>
      <div className="modal-actions wrap">
        <button className="small" onClick={() => {
          api.pgpExportPublic(k.fingerprint).then((t) => navigator.clipboard.writeText(t))
            .then(() => onDone("Skopiowano klucz publiczny – możesz go wkleić w wiadomości albo na stronie."))
            .catch(onFail);
        }}>Kopiuj klucz publiczny</button>
        <button className="small" onClick={() => { api.pgpSaveKey(k.fingerprint, false, null).then((r) => onDone(r.path ? `Zapisano: ${r.path}` : "Nie zapisano pliku.")).catch(onFail); }}>Zapisz do pliku</button>
        {k.secret && <button className="small" onClick={() => saveSecret(null)}>Kopia klucza prywatnego</button>}
        {confirmDel ? (
          <>
            <button className="small warn" onClick={() => { setConfirmDel(false); api.pgpDelete(k.fingerprint).then(() => onDone("Usunięto klucz.")).catch(onFail); }}>
              {k.secret ? "Usuń na zawsze – bez kopii stracisz dostęp do zaszyfrowanej poczty" : "Na pewno usunąć?"}
            </button>
            <button className="small" onClick={() => setConfirmDel(false)}>Anuluj</button>
          </>
        ) : (
          <button className="small" onClick={() => setConfirmDel(true)}>Usuń</button>
        )}
      </div>
      {backupAsk && (
        <form className="pgp-pass" onSubmit={(e) => { e.preventDefault(); if (pass) saveSecret(pass); }}>
          <input type="password" autoComplete="off" autoFocus placeholder="Hasło klucza" aria-label="Hasło klucza" value={pass} onChange={(e) => setPass(e.target.value)} />
          <button className="small primary" disabled={!pass}>Zapisz kopię</button>
          <button type="button" className="small" onClick={() => { setBackupAsk(false); setPass(""); }}>Anuluj</button>
        </form>
      )}
    </li>
  );
}

function Generate({ accounts, onDone, onFail }: { accounts: string[]; onDone: (m: string) => void; onFail: (e: unknown) => void }) {
  const [email, setEmail] = useState(accounts[0] ?? "");
  const [name, setName] = useState("");
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const [busy, setBusy] = useState(false);
  const [local, setLocal] = useState<string | null>(null);
  useEffect(() => { if (!accounts.includes(email)) setEmail(accounts[0] ?? ""); }, [accounts, email]);
  const go = () => {
    setLocal(null);
    if (p1.length < 8) { setLocal("Hasło klucza musi mieć co najmniej 8 znaków."); return; }
    if (p1 !== p2) { setLocal("Hasła się różnią."); return; }
    setBusy(true);
    api.pgpGenerate(name, email, p1)
      .then((f) => { setP1(""); setP2(""); onDone(`Utworzono klucz dla ${email} (odcisk ${fmtFpr(f)}). Zrób jego kopię zapasową.`); })
      .catch(onFail)
      .finally(() => setBusy(false));
  };
  return (
    <form className="pgp-gen" onSubmit={(e) => { e.preventDefault(); go(); }}>
      <p className="hint">Nowy klucz (Ed25519 / Curve25519, ważny 3 lata) dla konta:</p>
      <div className="check sound-row">
        <label htmlFor="pgp-acc">Adres</label>
        <select id="pgp-acc" value={email} onChange={(e) => setEmail(e.target.value)}>
          {accounts.map((a) => <option key={a} value={a}>{a}</option>)}
        </select>
      </div>
      <div className="check sound-row">
        <label htmlFor="pgp-name">Imię i nazwisko (widoczne w kluczu)</label>
        <input id="pgp-name" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} />
      </div>
      <div className="check sound-row">
        <label htmlFor="pgp-p1">Hasło klucza</label>
        <input id="pgp-p1" type="password" autoComplete="new-password" value={p1} onChange={(e) => setP1(e.target.value)} />
      </div>
      <div className="check sound-row">
        <label htmlFor="pgp-p2">Powtórz hasło</label>
        <input id="pgp-p2" type="password" autoComplete="new-password" value={p2} onChange={(e) => setP2(e.target.value)} />
      </div>
      <p className="hint">Hasła nie da się odzyskać. Bez niego nie odczytasz wiadomości zaszyfrowanych do tego klucza.</p>
      {local && <p className="error" role="alert">{local}</p>}
      <div className="modal-actions"><button className="primary" disabled={busy || !email}>{busy ? "Tworzenie klucza…" : "Utwórz klucz"}</button></div>
    </form>
  );
}

function Import({ onDone, onFail }: { onDone: (m: string) => void; onFail: (e: unknown) => void }) {
  const [text, setText] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);
  const report = (r: { fingerprints: string[]; newKeys: number; secret: number }) =>
    onDone(`Zaimportowano: ${r.fingerprints.map(fmtFpr).join("; ")}${r.secret > 0 ? " (z kluczem prywatnym)" : ""}. Porównaj odcisk z właścicielem klucza.`);
  const fromFile = (f: File | undefined) => {
    if (!f) return;
    if (f.size > 5 * 1024 * 1024) { onFail("Plik klucza jest zbyt duży."); return; }
    const fr = new FileReader();
    fr.onerror = () => onFail("Nie można odczytać pliku.");
    fr.onload = () => {
      const s = String(fr.result);
      api.pgpImport(null, s.slice(s.indexOf(",") + 1)).then(report).catch(onFail);
    };
    fr.readAsDataURL(f);
    if (fileRef.current) fileRef.current.value = "";
  };
  return (
    <div className="pgp-import">
      <label htmlFor="pgp-paste" className="hint">Import klucza (publicznego albo własnego prywatnego) – wklej tekst „-----BEGIN PGP … KEY BLOCK-----”:</label>
      <textarea id="pgp-paste" rows={4} spellCheck={false} value={text} onChange={(e) => setText(e.target.value)} />
      <div className="modal-actions wrap">
        <button disabled={!text.trim()} onClick={() => { api.pgpImport(text, null).then((r) => { setText(""); report(r); }).catch(onFail); }}>Importuj wklejony</button>
        <input ref={fileRef} type="file" accept=".asc,.gpg,.pgp,.key,application/pgp-keys" hidden onChange={(e) => fromFile(e.target.files?.[0])} />
        <button onClick={() => fileRef.current?.click()}>Importuj z pliku…</button>
      </div>
    </div>
  );
}
