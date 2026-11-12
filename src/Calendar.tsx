import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errText } from "./api";
import { holidayOn, isWeekend } from "./holidays";
import { occurrences, type Occurrence } from "./recur";
import type { CalEvent } from "./types";

const DAY = 86_400_000;
const MONTHS = ["styczeń", "luty", "marzec", "kwiecień", "maj", "czerwiec", "lipiec", "sierpień", "wrzesień", "październik", "listopad", "grudzień"];
const WEEK = ["pon", "wt", "śr", "czw", "pt", "sob", "niedz"];
const REMIND: [number, string][] = [
  [-1, "bez przypomnienia"], [0, "w chwili rozpoczęcia"], [5, "5 minut wcześniej"], [15, "15 minut wcześniej"],
  [30, "30 minut wcześniej"], [60, "godzinę wcześniej"], [1440, "dzień wcześniej"],
];

const pad = (n: number) => String(n).padStart(2, "0");
const dateStr = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const timeStr = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
/** Początek doby w czasie lokalnym (bez zgadywania przesunięć: składamy datę z części). */
const dayStart = (y: number, m: number, d: number) => new Date(y, m, d, 0, 0, 0, 0);
const parseLocal = (date: string, time: string) => {
  const [y, mo, d] = date.split("-").map(Number);
  const [h, mi] = time.split(":").map(Number);
  return new Date(y, mo - 1, d, h || 0, mi || 0, 0, 0);
};

function dayOccurrences(events: CalEvent[], day: Date): Occurrence[] {
  const s = dayStart(day.getFullYear(), day.getMonth(), day.getDate()).getTime();
  const next = dayStart(day.getFullYear(), day.getMonth(), day.getDate() + 1).getTime();
  return events.flatMap((e) => occurrences(e, s, next)).sort((a, b) => a.startMs - b.startMs);
}

const REPEAT: [string, string][] = [
  ["none", "nie powtarza się"], ["daily", "codziennie"], ["weekly", "co tydzień"], ["monthly", "co miesiąc"], ["yearly", "co rok"],
];

/** Dane do wstępnego wypełnienia formularza nowego wydarzenia (np. z wiadomości). */
export interface CalPrefill { title: string; notes: string; date: string; time?: string }

interface Form {
  id?: number;
  title: string;
  date: string;
  allDay: boolean;
  start: string;
  end: string;
  remind: number;
  notes: string;
  repeat: string;
  until: string;
}

export function CalendarDialog({ onClose, prefill, standalone }: { onClose: () => void; prefill?: CalPrefill | null; standalone?: boolean }) {
  const today = new Date();
  const [cursor, setCursor] = useState(() => {
    const d = prefill ? parseLocal(prefill.date, "00:00") : today;
    return new Date(d.getFullYear(), d.getMonth(), 1);
  });
  const [sel, setSel] = useState(() => {
    const d = prefill ? parseLocal(prefill.date, "00:00") : today;
    return dayStart(d.getFullYear(), d.getMonth(), d.getDate());
  });
  const [events, setEvents] = useState<CalEvent[]>([]);
  const [form, setForm] = useState<Form | null>(() => {
    if (!prefill) return null;
    const t = prefill.time ?? "09:00";
    const h = Number(t.slice(0, 2));
    return {
      title: prefill.title, date: prefill.date, allDay: !prefill.time, start: t, end: `${pad(Math.min(h + 1, 23))}${t.slice(2)}`,
      remind: 15, notes: prefill.notes, repeat: "none", until: "",
    };
  });
  const [err, setErr] = useState<string | null>(null);
  const [confirmDel, setConfirmDel] = useState<number | null>(null);

  // Siatka miesiąca: 6 tygodni zaczynających się w poniedziałek.
  const cells = useMemo(() => {
    const first = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
    const lead = (first.getDay() + 6) % 7;
    return Array.from({ length: 42 }, (_, i) => new Date(cursor.getFullYear(), cursor.getMonth(), 1 - lead + i));
  }, [cursor]);

  const load = useCallback(() => {
    const from = cells[0].getTime();
    const to = new Date(cells[41].getFullYear(), cells[41].getMonth(), cells[41].getDate() + 1).getTime();
    api.calRange(from, to).then((l) => { setEvents(l); setErr(null); }).catch((e) => setErr(errText(e)));
  }, [cells]);
  useEffect(load, [load]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { if (form) setForm(null); else onClose(); } };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [form, onClose]);

  const dayEvents = (d: Date) => dayOccurrences(events, d);
  const selEvents = dayEvents(sel);

  const newEvent = () => {
    const now = new Date();
    const h = sel.toDateString() === now.toDateString() ? Math.min(now.getHours() + 1, 22) : 9;
    setForm({ title: "", date: dateStr(sel), allDay: false, start: `${pad(h)}:00`, end: `${pad(h + 1)}:00`, remind: 15, notes: "", repeat: "none", until: "" });
  };
  const edit = (e: CalEvent) => {
    const s = new Date(e.startMs);
    const en = new Date(e.endMs);
    setForm({
      id: e.id, title: e.title, date: dateStr(s), allDay: e.allDay, start: timeStr(s), end: timeStr(en),
      remind: e.remindMin, notes: e.notes, repeat: e.repeat || "none",
      until: e.untilMs ? dateStr(new Date(e.untilMs)) : "",
    });
  };

  const save = async () => {
    if (!form) return;
    setErr(null);
    let startMs: number;
    let endMs: number;
    if (form.allDay) {
      const s = parseLocal(form.date, "00:00");
      startMs = s.getTime();
      endMs = startMs + DAY - 1;
    } else {
      startMs = parseLocal(form.date, form.start).getTime();
      endMs = parseLocal(form.date, form.end).getTime();
    }
    if (Number.isNaN(startMs) || Number.isNaN(endMs)) { setErr("Podaj poprawną datę i godzinę."); return; }
    if (endMs < startMs) { setErr("Koniec nie może być przed początkiem."); return; }
    try {
      const untilMs = form.repeat !== "none" && form.until ? parseLocal(form.until, "23:59").getTime() : 0;
      await api.calSave({ id: form.id, title: form.title, notes: form.notes, startMs, endMs, allDay: form.allDay, remindMin: form.remind, repeat: form.repeat, untilMs });
      setForm(null);
      const d = parseLocal(form.date, "00:00");
      setSel(d);
      setCursor(new Date(d.getFullYear(), d.getMonth(), 1));
      load();
    } catch (e) { setErr(errText(e)); }
  };

  const remove = async (e: CalEvent) => {
    if (e.id === undefined) return;
    setConfirmDel(null);
    try { await api.calDelete(e.id); load(); } catch (x) { setErr(errText(x)); }
  };

  const go = (delta: number) => setCursor(new Date(cursor.getFullYear(), cursor.getMonth() + delta, 1));
  const goToday = () => {
    setCursor(new Date(today.getFullYear(), today.getMonth(), 1));
    setSel(dayStart(today.getFullYear(), today.getMonth(), today.getDate()));
  };
  const timeLabel = (o: Occurrence) => o.event.allDay ? "cały dzień" : `${timeStr(new Date(o.startMs))}–${timeStr(new Date(o.endMs))}`;

  return (
    <div className={standalone ? "cal-page" : "modal-back"} role={standalone ? undefined : "dialog"} aria-modal={standalone ? undefined : true} aria-label="Kalendarz">
      <div className={standalone ? "cal-modal cal-pagebox" : "modal cal-modal"}>
        <div className="cal-head">
          <button className="small" aria-label="Poprzedni miesiąc" onClick={() => go(-1)}>‹</button>
          <h3>{MONTHS[cursor.getMonth()]} {cursor.getFullYear()}</h3>
          <button className="small" aria-label="Następny miesiąc" onClick={() => go(1)}>›</button>
          <button className="small" onClick={goToday}>Dziś</button>
          <span className="grow" />
          <button onClick={onClose}>Zamknij</button>
        </div>
        <p className="hint">Czerwone dni to polskie święta wolne od pracy (liczone w programie, także Wielkanoc i Boże Ciało), szare to weekendy. Kalendarz jest lokalny: zapisany tylko na tym komputerze, niepołączony z pocztą ani z żadnym kontem. Wchodzi do kopii zapasowej (Ustawienia → Poczta).</p>
        <div key={`${cursor.getFullYear()}-${cursor.getMonth()}`} className="cal-grid" role="grid" aria-label="Dni miesiąca">
          {WEEK.map((w) => <div key={w} className="cal-dow" role="columnheader">{w}</div>)}
          {cells.map((d) => {
            const n = dayEvents(d).length;
            const hol = holidayOn(d);
            const cls = "cal-day" + (hol ? " holiday" : isWeekend(d) ? " weekend" : "") + (d.getMonth() !== cursor.getMonth() ? " other" : "") +
              (d.toDateString() === today.toDateString() ? " today" : "") + (d.toDateString() === sel.toDateString() ? " sel" : "");
            return (
              <button key={d.getTime()} className={cls} role="gridcell" aria-label={`${d.getDate()} ${MONTHS[d.getMonth()]}${hol ? `, ${hol.name}` : ""}${n ? `, wydarzeń: ${n}` : ""}`} title={hol ? `${hol.name}: ${hol.desc}` : undefined}
                onClick={() => { setSel(d); setForm(null); }} onDoubleClick={() => { setSel(d); newEvent(); }}>
                <span>{d.getDate()}</span>
                {hol && <span className="cal-hol">{hol.name}</span>}
                {n > 0 && <span className="cal-dot" aria-hidden="true">{n > 1 ? n : ""}</span>}
              </button>
            );
          })}
        </div>
        <div className="cal-side">
          <h4>{sel.getDate()} {MONTHS[sel.getMonth()]} {sel.getFullYear()}</h4>
          {holidayOn(sel) && (
            <div className="cal-holcard" role="note">
              <b>{holidayOn(sel)?.name}</b> · dzień wolny od pracy
              <p>{holidayOn(sel)?.desc}</p>
            </div>
          )}
          {!holidayOn(sel) && isWeekend(sel) && <p className="hint">Weekend – dzień wolny.</p>}
          {selEvents.length === 0 && !form && <p className="hint">Brak wydarzeń tego dnia.</p>}
          {!form && (
            <ul className="addr-list">
              {selEvents.map((o) => {
                const e = o.event;
                return (
                  <li key={`${e.id}-${o.startMs}`}>
                    <span title={e.notes || undefined}><b>{timeLabel(o)}</b> {e.title}{e.remindMin >= 0 ? " 🔔" : ""}{e.repeat !== "none" ? " ↻" : ""}</span>
                    <span>
                      {confirmDel === e.id ? (
                        <>
                          <span className="warn-line">{e.repeat !== "none" ? "Usunąć całą serię bezpowrotnie?" : "Usunąć bezpowrotnie?"}</span>{" "}
                          <button className="small solid" onClick={() => void remove(e)}>Usuń</button>{" "}
                          <button className="small" onClick={() => setConfirmDel(null)}>Anuluj</button>
                        </>
                      ) : (
                        <>
                          <button className="small" onClick={() => edit(e)}>{e.repeat !== "none" ? "Edytuj serię" : "Edytuj"}</button>{" "}
                          <button className="small" onClick={() => setConfirmDel(e.id ?? null)}>Usuń</button>
                        </>
                      )}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
          {!form && <button onClick={newEvent}>Dodaj wydarzenie</button>}
          {form && (
            <div className="cal-form">
              <input value={form.title} maxLength={120} placeholder="Tytuł" aria-label="Tytuł wydarzenia" autoFocus
                onChange={(e) => setForm({ ...form, title: e.target.value })} />
              <label className="cal-row"><input type="checkbox" checked={form.allDay} onChange={(e) => setForm({ ...form, allDay: e.target.checked })} /> Cały dzień</label>
              <div className="cal-row">
                <input type="date" value={form.date} aria-label="Data" onChange={(e) => setForm({ ...form, date: e.target.value })} />
                {!form.allDay && (
                  <>
                    <input type="time" value={form.start} aria-label="Początek" onChange={(e) => setForm({ ...form, start: e.target.value })} />
                    <span>–</span>
                    <input type="time" value={form.end} aria-label="Koniec" onChange={(e) => setForm({ ...form, end: e.target.value })} />
                  </>
                )}
              </div>
              <select value={form.remind} aria-label="Przypomnienie" onChange={(e) => setForm({ ...form, remind: Number(e.target.value) })}>
                {REMIND.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
              </select>
              <div className="cal-row">
                <select value={form.repeat} aria-label="Powtarzanie" onChange={(e) => setForm({ ...form, repeat: e.target.value })}>
                  {REPEAT.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                </select>
                {form.repeat !== "none" && (
                  <label className="cal-row">do <input type="date" value={form.until} aria-label="Powtarzaj do (puste = bez końca)" onChange={(e) => setForm({ ...form, until: e.target.value })} /></label>
                )}
              </div>
              <textarea rows={3} maxLength={2000} value={form.notes} placeholder="Notatki (opcjonalnie)" aria-label="Notatki"
                onChange={(e) => setForm({ ...form, notes: e.target.value })} />
              <div className="cal-row">
                <button className="primary" onClick={() => void save()}>Zapisz</button>
                <button onClick={() => setForm(null)}>Anuluj</button>
              </div>
            </div>
          )}
          {err && <p className="error" role="alert">{err}</p>}
        </div>
      </div>
    </div>
  );
}
