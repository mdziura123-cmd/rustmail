import { useEffect, useState } from "react";
import { api, errText } from "./api";
import { ActIcon } from "./ActIcon";

/** Książka adresów w Ustawieniach: podgląd, dodawanie ręczne, usuwanie i czyszczenie. Dane są tylko na tym komputerze. */
export function AddressBook() {
  const [list, setList] = useState<{ email: string; name: string }[] | null>(null);
  const [add, setAdd] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const load = () => api.contactsList().then(setList).catch((e) => setErr(errText(e)));
  useEffect(() => { void load(); }, []);
  return (
    <div className="addr-book">
      <h4>Książka adresów</h4>
      <p className="hint">
        Adresy, do których wysłałeś wiadomość, zapisują się same i podpowiadają przy pisaniu. Są tylko na tym komputerze, w zaszyfrowanej bazie.
      </p>
      <form className="addr-add" onSubmit={(e) => {
        e.preventDefault();
        if (!add.trim()) return;
        api.contactsRemember(add).then((n) => {
          if (n === 0) setErr("Nie znalazłem poprawnego adresu e-mail."); else { setErr(null); setAdd(""); void load(); }
        }).catch((x) => setErr(errText(x)));
      }}>
        <input value={add} onChange={(e) => setAdd(e.target.value)} placeholder="Dodaj adres, np. Jan Kowalski <jan@firma.pl>" aria-label="Dodaj adres" />
        <button className="small" type="submit" title="Dodaj do książki adresów" aria-label="Dodaj do książki adresów"><ActIcon n="plus" /></button>
      </form>
      {err && <p className="error" role="alert">{err}</p>}
      {list && list.length === 0 && <p className="hint">Książka adresów jest pusta. Wyślij pierwszą wiadomość, a adres zapisze się sam.</p>}
      {list && list.length > 0 && (
        <>
          <ul className="addr-list">
            {list.map((c) => (
              <li key={c.email}>
                <span className="addr-name">{c.name || c.email}</span>
                {c.name && <span className="addr-mail">{c.email}</span>}
                <button className="small" title={`Usuń ${c.email}`} aria-label={`Usuń ${c.email}`}
                  onClick={() => api.contactsDelete(c.email).then(load).catch((x) => setErr(errText(x)))}><ActIcon n="trash" /></button>
              </li>
            ))}
          </ul>
          {confirm ? (
            <p>
              Usunąć wszystkie adresy ({list.length})? To nieodwracalne.{" "}
              <button className="small warn" onClick={() => api.contactsClear().then(() => { setConfirm(false); void load(); }).catch((x) => setErr(errText(x)))}>Tak, usuń</button>{" "}
              <button className="small" onClick={() => setConfirm(false)}>Anuluj</button>
            </p>
          ) : <button className="small" onClick={() => setConfirm(true)}>Wyczyść książkę adresów</button>}
        </>
      )}
    </div>
  );
}
