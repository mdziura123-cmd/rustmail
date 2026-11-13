import { emojiSrc } from "./emoji";

/** Zwykły tekst, w którym znaczniki ⟦e:kod⟧ są pokazane jako obrazki emotek (tylko z własnego zestawu). */
export function EmojiText({ text }: { text: string }) {
  const parts = text.split(/⟦e:([0-9a-f]{2,6})⟧/);
  return (
    <>
      {parts.map((p, i) => {
        if (i % 2 === 0) return p;
        const src = emojiSrc(p);
        return src ? <img key={i} className="emo" src={src} alt="" draggable={false} /> : null;
      })}
    </>
  );
}
