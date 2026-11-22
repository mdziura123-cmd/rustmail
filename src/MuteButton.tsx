import { useEffect, useState } from "react";
import { api, errText } from "./api";
import { ActIcon } from "./ActIcon";

interface Props {
  /** g = wątek Gmaila (item = identyfikator wątku), i = konto IMAP (item = temat rozmowy). */
  kind: "g" | "i";
  account: string;
  item: string;
  onToast: (text: string) => void;
}

/** Przycisk wyciszenia rozmowy: bez powiadomień o kolejnych wiadomościach w tej rozmowie. */
export function MuteButton({ kind, account, item, onToast }: Props) {
  const [muted, setMuted] = useState(false);
  useEffect(() => {
    let live = true;
    setMuted(false);
    if (!account || !item) return;
    api.muteStatus(kind, account, [item]).then((r) => { if (live) setMuted(r.includes(item)); }).catch(() => undefined);
    return () => { live = false; };
  }, [kind, account, item]);
  if (!account || !item) return null;
  const label = muted ? "Wyłącz wyciszenie rozmowy" : "Wycisz rozmowę – bez powiadomień o nowych odpowiedziach";
  const toggle = async () => {
    try {
      await api.muteThread(kind, account, item, !muted);
      setMuted(!muted);
      onToast(muted ? "Rozmowa nie jest już wyciszona." : kind === "i"
        ? "Rozmowa wyciszona – wiadomości o tym temacie nie będą powiadamiać."
        : "Rozmowa wyciszona – nowe odpowiedzi nie będą powiadamiać.");
    } catch (e) {
      onToast(errText(e));
    }
  };
  return (
    <button title={label} aria-label={label} aria-pressed={muted} onClick={() => void toggle()}>
      <ActIcon n={muted ? "mute" : "bell"} />
    </button>
  );
}
