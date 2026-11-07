import { useState } from "react";
import { api, errText } from "./api";

interface Props {
  notify: boolean;
  content: boolean;
  onNotify: (on: boolean) => void;
  onContent: (on: boolean) => void;
  onOpen: (what: "signature" | "senders" | "offline") => void;
  onClose: () => void;
}

export function SettingsDialog({ notify, content, onNotify, onContent, onOpen, onClose }: Props) {
  const [msg, setMsg] = useState<string | null>(null);
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
      <div className="modal">
        <h3>Ustawienia</h3>
        <label className="check">
          <input type="checkbox" checked={notify} onChange={(e) => onNotify(e.target.checked)} />
          Powiadomienia pulpitowe o nowych wiadomościach
        </label>
        <label className="check">
          <input type="checkbox" checked={content} disabled={!notify} onChange={(e) => onContent(e.target.checked)} />
          Pokazuj w powiadomieniu nadawcę i temat
        </label>
        <p className="hint">
          Domyślnie powiadomienie mówi tylko, że przyszła nowa poczta. Treść wiadomości z ostrzeżeniem nigdy nie jest pokazywana.
          Poczta jest sprawdzana co minutę, gdy aplikacja jest uruchomiona; powiadomienia wymagają programu
          <code> notify-send</code> w systemie. Wiadomość, która dojdzie do już nieprzeczytanego wątku, nie wywoła powiadomienia.
        </p>
        <div className="modal-actions">
          <button disabled={!notify} onClick={() => void test()}>Wyślij próbne powiadomienie</button>
        </div>
        {msg && <p className="hint" role="status">{msg}</p>}
        <div className="modal-actions">
          <button onClick={() => onOpen("signature")}>Podpis…</button>
          <button onClick={() => onOpen("senders")}>Listy nadawców…</button>
          <button onClick={() => onOpen("offline")}>Tryb offline…</button>
        </div>
        <div className="modal-actions">
          <button className="primary" onClick={onClose}>Zamknij</button>
        </div>
      </div>
    </div>
  );
}
