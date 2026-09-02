import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, MouseEvent } from "react";
import { api } from "../../lib/api";
import type { planner } from "../../lib/api";
import type { CalendarEvent, CalendarEventInput, EventReminder } from "../../lib/types";
import { useAppStore } from "../../store/appStore";

/**
 * 📅 Calendar — month / week / day views over the local `events` table,
 * with client-side recurrence expansion, reminders, search and Planner
 * todo integration. Fully offline; all data comes from the Tauri backend.
 */

// ─── constants ──────────────────────────────────────────────────────────────

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAY_MS = 24 * 60 * 60 * 1000;
const HOUR_H = 44; // px per hour in the time grid
const EXPAND_CAP = 5000; // safety cap for recurrence iteration

const CATEGORIES = ["workshop", "hackathon", "focus", "general", "exam"];
const CATEGORY_COLORS: Record<string, string> = {
  workshop: "#f43f5e",
  hackathon: "#8b5cf6",
  focus: "#38bdf8",
  general: "#34d399",
  exam: "#f59e0b",
};
const RECURRENCES: Array<[string, string]> = [
  ["none", "Never"],
  ["daily", "Daily"],
  ["weekly", "Weekly"],
  ["monthly", "Monthly"],
  ["yearly", "Yearly"],
];

const evColor = (ev: CalendarEvent) =>
  ev.color || CATEGORY_COLORS[ev.category] || "#34d399";

const todoColor = (t: planner.Todo) =>
  t.completed
    ? "#5d6779"
    : t.priority === "high"
      ? "#f43f5e"
      : t.priority === "medium"
        ? "#fbbf24"
        : "#38bdf8";

// ─── date helpers ───────────────────────────────────────────────────────────

const pad = (n: number) => String(n).padStart(2, "0");
const dstr = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const tstr = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
const sameDay = (a: Date, b: Date) => dstr(a) === dstr(b);
const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const startOfWeek = (d: Date) => addDays(startOfDay(d), -((d.getDay() + 6) % 7)); // Monday-start

function addDays(d: Date, n: number) {
  const x = new Date(d);
  x.setDate(x.getDate() + n);
  return x;
}

function addMonths(d: Date, n: number) {
  const x = new Date(d);
  const day = x.getDate();
  x.setDate(1);
  x.setMonth(x.getMonth() + n);
  x.setDate(Math.min(day, new Date(x.getFullYear(), x.getMonth() + 1, 0).getDate()));
  return x;
}

const addYears = (d: Date, n: number) => addMonths(d, n * 12);

function nextHour(d: Date) {
  const x = new Date(d);
  x.setMinutes(0, 0, 0);
  x.setHours(x.getHours() + 1);
  return x;
}

// ─── recurrence expansion ──────────────────────────────────────────────────

interface Occ {
  ev: CalendarEvent;
  start: Date;
  end: Date;
}

/** Expand an event into concrete occurrences overlapping `[rangeStart, rangeEnd]`. */
function expandOccurrences(ev: CalendarEvent, rangeStart: Date, rangeEnd: Date): Occ[] {
  const baseStart = new Date(ev.startAt);
  const baseEnd = new Date(ev.endAt);
  if (isNaN(baseStart.getTime()) || isNaN(baseEnd.getTime())) return [];
  const durMs = Math.max(baseEnd.getTime() - baseStart.getTime(), 0);
  const recEndStr =
    ev.recurrenceEnd && ev.recurrenceEnd.length >= 10 ? ev.recurrenceEnd.slice(0, 10) : null;
  const rs = rangeStart.getTime();
  const re = rangeEnd.getTime();
  const out: Occ[] = [];

  // Returns false when scanning should stop (past range end or recurrence end).
  const push = (s: Date): boolean => {
    if (s.getTime() > re) return false;
    if (recEndStr && dstr(s) > recEndStr) return false;
    const e = s.getTime() + durMs;
    if (e >= rs) out.push({ ev, start: s, end: new Date(e) });
    return true;
  };

  switch (ev.recurrence) {
    case "daily":
    case "weekly": {
      const period = ev.recurrence === "daily" ? DAY_MS : 7 * DAY_MS;
      const firstK = Math.max(
        0,
        Math.floor((rs - durMs - baseStart.getTime()) / period) - 1,
      );
      for (let k = firstK; k < firstK + EXPAND_CAP; k++) {
        if (!push(new Date(baseStart.getTime() + k * period))) break;
      }
      break;
    }
    case "monthly": {
      const monthsBase = baseStart.getFullYear() * 12 + baseStart.getMonth();
      const monthsRange = rangeStart.getFullYear() * 12 + rangeStart.getMonth();
      const durMonths = Math.ceil(durMs / (30 * DAY_MS)) + 1;
      let i = Math.max(0, monthsRange - monthsBase - durMonths);
      let count = 0;
      while (count++ < EXPAND_CAP && push(addMonths(baseStart, i))) i++;
      break;
    }
    case "yearly": {
      const yearsBase = baseStart.getFullYear();
      const yearsRange = rangeStart.getFullYear();
      const durYears = Math.ceil(durMs / (365 * DAY_MS)) + 1;
      let i = Math.max(0, yearsRange - yearsBase - durYears);
      let count = 0;
      while (count++ < EXPAND_CAP && push(addYears(baseStart, i))) i++;
      break;
    }
    default: {
      if (baseEnd.getTime() >= rs && baseStart.getTime() <= re) {
        out.push({ ev, start: baseStart, end: baseEnd });
      }
    }
  }
  return out;
}

/** Greedy lane assignment so overlapping events sit side by side. */
function assignLanes(occs: Occ[]) {
  const sorted = [...occs].sort(
    (a, b) => a.start.getTime() - b.start.getTime() || b.end.getTime() - a.end.getTime(),
  );
  const laneEnds: number[] = [];
  const placed: Array<{ occ: Occ; lane: number; lanes: number }> = [];
  for (const occ of sorted) {
    let lane = laneEnds.findIndex((end) => end <= occ.start.getTime());
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(occ.end.getTime());
    } else {
      laneEnds[lane] = occ.end.getTime();
    }
    placed.push({ occ, lane, lanes: 0 });
  }
  for (const p of placed) p.lanes = Math.max(laneEnds.length, 1);
  return placed;
}

const isGridTimed = (o: Occ) => !o.ev.isAllDay && sameDay(o.start, o.end);

function formatWhen(ev: CalendarEvent) {
  const s = new Date(ev.startAt);
  return ev.isAllDay
    ? s.toLocaleDateString(undefined, { month: "short", day: "numeric" })
    : s.toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
}

// ─── dialog state ──────────────────────────────────────────────────────────

interface DialogState {
  event: CalendarEvent | null;
  presetStart: Date | null;
  presetEnd: Date | null;
}

// ─── event create/edit dialog ──────────────────────────────────────────────

function EventDialog({
  state,
  onClose,
  onSaved,
}: {
  state: DialogState;
  onClose: () => void;
  onSaved: () => void;
}) {
  const showToast = useAppStore((s) => s.showToast);
  const ev = state.event;
  const isEdit = !!ev;
  const allDay0 = ev ? ev.isAllDay : false;
  const s0 = ev ? new Date(ev.startAt) : (state.presetStart ?? nextHour(new Date()));
  const e0 = ev
    ? new Date(ev.endAt)
    : (state.presetEnd ?? new Date(s0.getTime() + 60 * 60 * 1000));

  const [title, setTitle] = useState(ev?.title ?? "");
  const [description, setDescription] = useState(ev?.description ?? "");
  const [startDate, setStartDate] = useState(dstr(s0));
  const [startTime, setStartTime] = useState(allDay0 ? "09:00" : tstr(s0));
  const [endDate, setEndDate] = useState(dstr(e0));
  const [endTime, setEndTime] = useState(allDay0 ? "10:00" : tstr(e0));
  const [isAllDay, setIsAllDay] = useState(allDay0);
  const [category, setCategory] = useState(ev?.category ?? "general");
  const [color, setColor] = useState<string | null>(ev?.color ?? null);
  const [location, setLocation] = useState(ev?.location ?? "");
  const [sourceUrl, setSourceUrl] = useState(ev?.sourceUrl ?? "");
  const [certificateOffered, setCertificateOffered] = useState(!!ev?.certificateOffered);
  const [registrationRequired, setRegistrationRequired] = useState(
    !!ev?.registrationRequired,
  );
  const [recurrence, setRecurrence] = useState(ev?.recurrence ?? "none");
  const [recurrenceEnd, setRecurrenceEnd] = useState(
    ev?.recurrenceEnd && ev.recurrenceEnd.length >= 10 ? ev.recurrenceEnd.slice(0, 10) : "",
  );
  const [remindersText, setRemindersText] = useState("");
  const [remindersDirty, setRemindersDirty] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [delBusy, setDelBusy] = useState(false);

  useEffect(() => {
    if (!ev) return;
    api
      .listEventReminders(ev.id)
      .then((rems: EventReminder[]) =>
        setRemindersText(rems.map((r) => r.minutesBefore).join(", ")),
      )
      .catch(() => {});
  }, [ev]);

  const buildPayload = (): CalendarEventInput | null => {
    const t = title.trim();
    if (!t) {
      setError("Title is required.");
      return null;
    }
    const startAt = isAllDay
      ? new Date(`${startDate}T00:00`).toISOString()
      : new Date(`${startDate}T${startTime || "09:00"}`).toISOString();
    const endAt = isAllDay
      ? new Date(`${endDate}T23:59`).toISOString()
      : new Date(`${endDate}T${endTime || "10:00"}`).toISOString();
    const sMs = new Date(startAt).getTime();
    const eMs = new Date(endAt).getTime();
    if (isNaN(sMs) || isNaN(eMs)) {
      setError("Invalid date or time.");
      return null;
    }
    if (eMs < sMs) {
      setError(isAllDay ? "End date can't be before start date." : "End must be after start.");
      return null;
    }
    setError("");

    const mins = remindersText
      .split(",")
      .map((x) => parseInt(x.trim(), 10))
      .filter((n) => !isNaN(n) && n >= 0 && n < 10_000);

    const payload: CalendarEventInput = {
      title: t,
      description: description.trim() || null,
      startAt,
      endAt,
      category,
      location: location.trim() || null,
      sourceUrl: sourceUrl.trim() || null,
      isAllDay,
      certificateOffered,
      registrationRequired,
      recurrence,
      recurrenceEnd: recurrence === "none" ? null : recurrenceEnd || null,
      color,
    };
    // Only touch reminders when creating or when the user actually edited
    // them — backend `Some` deletes + re-inserts, `None` keeps existing rows.
    if (!ev || remindersDirty) payload.reminderMinutes = mins;
    return payload;
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const payload = buildPayload();
    if (!payload) return;
    setBusy(true);
    try {
      if (ev) {
        await api.updateCalendarEvent(ev.id, payload);
        showToast("success", "Event updated");
      } else {
        await api.createCalendarEvent(payload);
        showToast("success", "Event created");
      }
      onSaved();
    } catch (err) {
      showToast("error", typeof err === "string" ? err : String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!ev) return;
    setDelBusy(true);
    try {
      await api.deleteCalendarEvent(ev.id);
      showToast("success", "Event deleted");
      onSaved();
    } catch (err) {
      showToast("error", typeof err === "string" ? err : String(err));
      setDelBusy(false);
    }
  };

  const backdropDown = (e: MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget) onClose();
  };

  return (
    <div className="dialog-backdrop" onMouseDown={backdropDown}>
      <div className="dialog cal-dialog">
        <h3>{isEdit ? "✏️ Edit Event" : "➕ New Event"}</h3>
        <form onSubmit={submit}>
          <div className="field">
            <label>Title</label>
            <input
              type="text"
              value={title}
              autoFocus
              placeholder="What's happening?"
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>

          <div className="field">
            <label>{isAllDay ? "Dates" : "Date & time"}</label>
            <div className="cal-field-row">
              {isAllDay ? (
                <>
                  <input
                    type="date"
                    value={startDate}
                    onChange={(e) => setStartDate(e.target.value)}
                  />
                  <span className="text-dim">→</span>
                  <input
                    type="date"
                    value={endDate}
                    onChange={(e) => setEndDate(e.target.value)}
                  />
                </>
              ) : (
                <>
                  <input
                    type="datetime-local"
                    value={`${startDate}T${startTime}`}
                    onChange={(e) => {
                      const v = e.target.value;
                      if (v.length >= 16) {
                        setStartDate(v.slice(0, 10));
                        setStartTime(v.slice(11, 16));
                      }
                    }}
                  />
                  <span className="text-dim">→</span>
                  <input
                    type="datetime-local"
                    value={`${endDate}T${endTime}`}
                    onChange={(e) => {
                      const v = e.target.value;
                      if (v.length >= 16) {
                        setEndDate(v.slice(0, 10));
                        setEndTime(v.slice(11, 16));
                      }
                    }}
                  />
                </>
              )}
            </div>
            <label className="cal-check">
              <input
                type="checkbox"
                checked={isAllDay}
                onChange={(e) => setIsAllDay(e.target.checked)}
              />
              All-day event
            </label>
          </div>

          <div className="cal-field-cols">
            <div className="field">
              <label>Category</label>
              <select value={category} onChange={(e) => setCategory(e.target.value)}>
                {(CATEGORIES.includes(category)
                  ? CATEGORIES
                  : [category, ...CATEGORIES]
                ).map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label>Color</label>
              <div className="cal-swatches">
                <button
                  type="button"
                  className={`cal-swatch auto${color === null ? " sel" : ""}`}
                  onClick={() => setColor(null)}
                  title="Auto (from category)"
                >
                  A
                </button>
                {Object.entries(CATEGORY_COLORS).map(([name, c]) => (
                  <button
                    type="button"
                    key={name}
                    className={`cal-swatch${color === c ? " sel" : ""}`}
                    style={{ background: c }}
                    onClick={() => setColor(c)}
                    title={name}
                  />
                ))}
                <input
                  type="color"
                  className="cal-color-input"
                  value={color ?? CATEGORY_COLORS[category] ?? "#34d399"}
                  onChange={(e) => setColor(e.target.value)}
                  title="Custom color"
                />
              </div>
            </div>
          </div>

          <div className="cal-field-cols">
            <div className="field">
              <label>Repeat</label>
              <select
                value={recurrence}
                onChange={(e) => setRecurrence(e.target.value)}
              >
                {RECURRENCES.map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label>Repeat until {recurrence === "none" ? "" : "(optional)"}</label>
              <input
                type="date"
                value={recurrenceEnd}
                disabled={recurrence === "none"}
                onChange={(e) => setRecurrenceEnd(e.target.value)}
              />
            </div>
          </div>

          <div className="field">
            <label>Reminders (minutes before, comma separated)</label>
            <input
              type="text"
              value={remindersText}
              placeholder="e.g. 15, 60"
              onChange={(e) => {
                setRemindersText(e.target.value);
                setRemindersDirty(true);
              }}
            />
          </div>

          <div className="field">
            <label>Location</label>
            <input
              type="text"
              value={location}
              placeholder="Where? (optional)"
              onChange={(e) => setLocation(e.target.value)}
            />
          </div>

          <div className="field">
            <label>Source URL</label>
            <input
              type="text"
              value={sourceUrl}
              placeholder="https://… (optional)"
              onChange={(e) => {
                setSourceUrl(e.target.value);
                if (!isEdit) setRegistrationRequired(!!e.target.value.trim());
              }}
            />
          </div>

          <div className="cal-field-cols">
            <label className="cal-check">
              <input
                type="checkbox"
                checked={certificateOffered}
                onChange={(e) => setCertificateOffered(e.target.checked)}
              />
              🎓 Certificate offered
            </label>
            <label className="cal-check">
              <input
                type="checkbox"
                checked={registrationRequired}
                onChange={(e) => setRegistrationRequired(e.target.checked)}
              />
              📝 Registration required
            </label>
          </div>

          <div className="field">
            <label>Description</label>
            <textarea
              value={description}
              placeholder="Notes…"
              rows={4}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>

          {error && <div className="field-error">{error}</div>}

          <div className="dialog-actions cal-dialog-actions">
            {isEdit && (
              <button
                type="button"
                className="btn danger small"
                disabled={delBusy || busy}
                onClick={remove}
              >
                {delBusy ? "Deleting…" : "Delete"}
              </button>
            )}
            <button type="button" className="btn ghost" onClick={onClose}>
              Cancel
            </button>
            <button
              type="submit"
              className="btn primary"
              disabled={busy || delBusy || !title.trim()}
            >
              {busy ? "Saving…" : isEdit ? "Save Changes" : "Create Event"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ─── main view ──────────────────────────────────────────────────────────────

type ViewMode = "month" | "week" | "day";

const CAL_REM_PREFIX = "CAL_REM:";

function loadFiredReminderKeys(): Set<string> {
  try {
    return new Set(
      Object.keys(localStorage).filter((k) => k.startsWith(CAL_REM_PREFIX)),
    );
  } catch {
    return new Set();
  }
}

export function CalendarView() {
  const showToast = useAppStore((s) => s.showToast);
  const [viewMode, setViewMode] = useState<ViewMode>("month");
  const [anchor, setAnchor] = useState(() => new Date());
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [remindersByEvent, setRemindersByEvent] = useState<Map<string, EventReminder[]>>(
    new Map(),
  );
  const [todos, setTodos] = useState<planner.Todo[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<CalendarEvent[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [categoryFilter, setCategoryFilter] = useState("");
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [nowTick, setNowTick] = useState(() => Date.now());
  const firedReminders = useRef<Set<string>>(new Set());

  // ── data loading ──

  const refresh = useCallback(async () => {
    try {
      const evs = await api.listCalendarEvents();
      setEvents(evs);
      const remLists = await Promise.all(
        evs.map((ev) => api.listEventReminders(ev.id).catch(() => [] as EventReminder[])),
      );
      const m = new Map<string, EventReminder[]>();
      evs.forEach((ev, i) => m.set(ev.id, remLists[i]));
      setRemindersByEvent(m);
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  const refreshTodos = useCallback(async () => {
    try {
      setTodos(await api.getTodos());
    } catch {
      /* todos are a nice-to-have overlay — stay silent on failure */
    }
  }, []);

  useEffect(() => {
    void refresh();
    void refreshTodos();
  }, [refresh, refreshTodos]);

  useEffect(() => {
    firedReminders.current = loadFiredReminderKeys();
  }, []);

  useEffect(() => {
    const t = window.setInterval(() => setNowTick(Date.now()), 60_000);
    return () => window.clearInterval(t);
  }, []);

  // ── reminder poller (fires toasts for events starting soon) ──

  useEffect(() => {
    const fire = () => {
      const now = Date.now();
      if (events.length === 0) return;
      const winStart = new Date(now);
      const winEnd = new Date(now + DAY_MS);
      for (const ev of events) {
        const rems = remindersByEvent.get(ev.id);
        if (!rems || rems.length === 0) continue;
        for (const occ of expandOccurrences(ev, winStart, winEnd)) {
          for (const r of rems) {
            if (!r.enabled) continue;
            const fireAt = occ.start.getTime() - r.minutesBefore * 60 * 1000;
            if (now < fireAt || now > occ.start.getTime() + 2 * 60 * 1000) continue;
            const key = `${CAL_REM_PREFIX}${ev.id}|${r.minutesBefore}|${occ.start.toISOString()}`;
            if (firedReminders.current.has(key)) continue;
            firedReminders.current.add(key);
            try {
              localStorage.setItem(key, "1");
            } catch {
              /* private mode etc. — toast still fires this session */
            }
            const when = occ.start.toLocaleTimeString(undefined, {
              hour: "2-digit",
              minute: "2-digit",
            });
            showToast(
              "info",
              `⏰ ${ev.title} — starts at ${when}${r.minutesBefore > 0 ? ` (reminder: ${r.minutesBefore} min before)` : ""}`,
            );
          }
        }
      }
    };
    fire();
    const t = window.setInterval(fire, 30_000);
    return () => window.clearInterval(t);
  }, [events, remindersByEvent, showToast]);

  // ── search (debounced, backend FTS/LIKE) ──

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setSearchResults(null);
      return;
    }
    const t = window.setTimeout(async () => {
      setSearching(true);
      try {
        setSearchResults(await api.searchCalendarEvents(q));
      } catch {
        setSearchResults([]);
      } finally {
        setSearching(false);
      }
    }, 300);
    return () => window.clearTimeout(t);
  }, [query]);

  // ── visible range + occurrence expansion ──

  const anchorKey = dstr(anchor);
  const view = useMemo(() => {
    const a = new Date(anchor);
    if (viewMode === "month") {
      const first = new Date(a.getFullYear(), a.getMonth(), 1);
      const gridStart = startOfWeek(first);
      return {
        start: gridStart,
        end: addDays(gridStart, 43),
        cells: Array.from({ length: 42 }, (_, i) => addDays(gridStart, i)),
        days: [] as Date[],
      };
    }
    if (viewMode === "week") {
      const ws = startOfWeek(a);
      return {
        start: ws,
        end: addDays(ws, 7),
        cells: [] as Date[],
        days: Array.from({ length: 7 }, (_, i) => addDays(ws, i)),
      };
    }
    const d = startOfDay(a);
    return { start: d, end: addDays(d, 1), cells: [] as Date[], days: [d] };
  }, [viewMode, anchorKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const occurrences = useMemo(() => {
    const list: Occ[] = [];
    for (const ev of events) {
      if (categoryFilter && ev.category !== categoryFilter) continue;
      for (const o of expandOccurrences(ev, view.start, view.end)) list.push(o);
    }
    list.sort((a, b) => a.start.getTime() - b.start.getTime());
    return list;
  }, [events, categoryFilter, view]);

  const occsOnDay = (day: Date) => {
    const ds = dstr(day);
    return occurrences.filter((o) => dstr(o.start) <= ds && dstr(o.end) >= ds);
  };

  const todosOnDay = (day: Date) => {
    const ds = dstr(day);
    return todos.filter((t) => t.dueDate === ds);
  };

  // ── navigation ──

  const prev = () =>
    setAnchor((a) =>
      viewMode === "month" ? addMonths(a, -1) : addDays(a, viewMode === "week" ? -7 : -1),
    );
  const next = () =>
    setAnchor((a) =>
      viewMode === "month" ? addMonths(a, 1) : addDays(a, viewMode === "week" ? 7 : 1),
    );
  const goToday = () => setAnchor(new Date());

  const title =
    viewMode === "month"
      ? anchor.toLocaleDateString(undefined, { month: "long", year: "numeric" })
      : viewMode === "week"
        ? `${view.days[0].toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${view.days[6].toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`
        : anchor.toLocaleDateString(undefined, {
            weekday: "long",
            month: "long",
            day: "numeric",
            year: "numeric",
          }) + (sameDay(anchor, new Date()) ? " · Today" : "");

  // ── actions ──

  const openCreateAt = (start: Date, end: Date | null) =>
    setDialog({ event: null, presetStart: start, presetEnd: end });

  const openEdit = (ev: CalendarEvent) => setDialog({ event: ev, presetStart: null, presetEnd: null });

  const toggleTodo = async (t: planner.Todo) => {
    try {
      const completed = await api.toggleTodo(t.id);
      await refreshTodos();
      showToast("success", `Task marked ${completed ? "done" : "open"}`);
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  // ── shared render helpers ──

  const eventChip = (occ: Occ, compact = true) => (
    <button
      key={`${occ.ev.id}@${occ.start.toISOString()}`}
      className={`cal-chip${occ.ev.isAllDay ? " allday" : ""}`}
      style={{ borderLeftColor: evColor(occ.ev) }}
      title={`${occ.ev.title}${occ.ev.isAllDay ? "" : ` · ${tstr(occ.start)}–${tstr(occ.end)}`}${occ.ev.recurrence !== "none" ? ` · repeats ${occ.ev.recurrence}` : ""}${occ.ev.location ? ` · ${occ.ev.location}` : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        openEdit(occ.ev);
      }}
    >
      {!occ.ev.isAllDay && (
        <span className="cal-chip-time">{tstr(occ.start)}</span>
      )}
      <span className="cal-chip-title">{compact ? occ.ev.title : `${occ.ev.title}`}</span>
    </button>
  );

  const todoChip = (t: planner.Todo) => (
    <button
      key={`todo-${t.id}`}
      className={`cal-chip todo${t.completed ? " done" : ""}`}
      style={{ borderLeftColor: todoColor(t) }}
      title={`${t.completed ? "☑" : "☐"} ${t.title} · Planner task${t.completed ? " (click to reopen)" : " (click to complete)"}`}
      onClick={(e) => {
        e.stopPropagation();
        void toggleTodo(t);
      }}
    >
      <span className="cal-chip-title">
        {t.completed ? "☑" : "☐"} {t.title}
      </span>
    </button>
  );

  const nowLineTop = (day: Date) => {
    const n = new Date(nowTick);
    if (!sameDay(n, day)) return null;
    return ((n.getHours() * 60 + n.getMinutes()) / 60) * HOUR_H;
  };

  // Time-grid day column (shared by week & day views).
  const timeColumn = (day: Date, key: string) => {
    const dayOccs = occsOnDay(day).filter(isGridTimed);
    const placed = assignLanes(dayOccs);
    const nl = nowLineTop(day);
    return (
      <div className="cal-daycol" key={key}>
        {Array.from({ length: 24 }, (_, h) => (
          <div
            key={h}
            className="cal-hourslot"
            style={{ top: h * HOUR_H, height: HOUR_H }}
            title={`New event at ${pad(h)}:00`}
            onClick={() => openCreateAt(
              new Date(day.getFullYear(), day.getMonth(), day.getDate(), h, 0),
              new Date(day.getFullYear(), day.getMonth(), day.getDate(), h + 1, 0),
            )}
          />
        ))}
        {placed.map(({ occ, lane, lanes }) => {
          const w = 100 / lanes;
          return (
            <button
              key={`${occ.ev.id}@${occ.start.toISOString()}`}
              className="cal-ev"
              style={{
                top: ((occ.start.getHours() * 60 + occ.start.getMinutes()) / 60) * HOUR_H,
                height: Math.max(((occ.end.getTime() - occ.start.getTime()) / 3_600_000) * HOUR_H - 3, 18),
                left: `calc(${lane * w}% + 2px)`,
                width: `calc(${w}% - 5px)`,
                borderLeftColor: evColor(occ.ev),
                background: `${evColor(occ.ev)}22`,
              }}
              title={`${occ.ev.title} · ${tstr(occ.start)}–${tstr(occ.end)}${occ.ev.location ? ` · ${occ.ev.location}` : ""}`}
              onClick={(e) => {
                e.stopPropagation();
                openEdit(occ.ev);
              }}
            >
              <span className="cal-ev-title">{occ.ev.title}</span>
              <span className="cal-ev-time">
                {tstr(occ.start)}–{tstr(occ.end)}
              </span>
            </button>
          );
        })}
        {nl !== null && (
          <div className="cal-now" style={{ top: nl }}>
            <span className="cal-now-dot" />
          </div>
        )}
      </div>
    );
  };

  const allDayRow = (day: Date, key: string) => {
    const allday = occsOnDay(day).filter((o) => !isGridTimed(o));
    const tds = todosOnDay(day);
    return (
      <div className="cal-adcell" key={key}>
        {allday.map((o) => eventChip(o))}
        {tds.map((t) => todoChip(t))}
      </div>
    );
  };

  const today = new Date();

  // ── render ──

  return (
    <div className="content">
      <div className="page-head">
        <div>
          <div className="dash-title">📅 Calendar</div>
          <div className="meta-line">
            {loading
              ? "Loading events…"
              : `${events.length} event${events.length === 1 ? "" : "s"} stored${categoryFilter ? ` · filtering: ${categoryFilter}` : ""}${todos.length ? ` · ${todos.filter((t) => t.dueDate).length} planner task${todos.filter((t) => t.dueDate).length === 1 ? "" : "s"} with due dates` : ""} · fully local`}
          </div>
        </div>
      </div>

      <div className="cal-toolbar">
        <div className="nav-tabs">
          {(["month", "week", "day"] as ViewMode[]).map((m) => (
            <button
              key={m}
              className={`nav-tab${viewMode === m ? " active" : ""}`}
              onClick={() => setViewMode(m)}
            >
              {m === "month" ? "Month" : m === "week" ? "Week" : "Day"}
            </button>
          ))}
        </div>

        <div className="cal-nav">
          <button className="btn ghost small" onClick={prev} title="Previous">
            ←
          </button>
          <button className="btn ghost small" onClick={goToday}>
            Today
          </button>
          <button className="btn ghost small" onClick={next} title="Next">
            →
          </button>
        </div>

        <div className="cal-title">{title}</div>

        <input
          className="cal-search"
          type="text"
          placeholder="Search events…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />

        <select
          className="cal-filter"
          value={categoryFilter}
          onChange={(e) => setCategoryFilter(e.target.value)}
          title="Filter by category"
        >
          <option value="">All categories</option>
          {CATEGORIES.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>

        <button
          className="btn primary small"
          onClick={() => openCreateAt(nextHour(new Date()), null)}
        >
          + New Event
        </button>
      </div>

      {searchResults && (
        <div className="cal-search-panel">
          <div className="section-label">
            {searching ? "Searching…" : `${searchResults.length} result${searchResults.length === 1 ? "" : "s"} for “${query.trim()}”`}
          </div>
          {searchResults.length === 0 && !searching && (
            <div className="empty-state">No events match that search.</div>
          )}
          {searchResults.map((ev) => (
            <button
              key={ev.id}
              className="cal-sr"
              onClick={() => {
                setSearchResults(null);
                setQuery("");
                const s = new Date(ev.startAt);
                setAnchor(s);
                openEdit(ev);
              }}
            >
              <span className="cal-dot" style={{ background: evColor(ev) }} />
              <span className="cal-sr-title">{ev.title}</span>
              <span className="cal-sr-when mono dim">{formatWhen(ev)}</span>
              {ev.recurrence !== "none" && (
                <span className="cal-sr-rec">🔁 {ev.recurrence}</span>
              )}
            </button>
          ))}
        </div>
      )}

      {/* ── Month ── */}
      {viewMode === "month" && (
        <div className="cal-month">
          <div className="cal-dow">
            {WEEKDAYS.map((w) => (
              <div key={w}>{w}</div>
            ))}
          </div>
          <div className="cal-grid">
            {view.cells.map((d) => {
              const dayOccs = occsOnDay(d);
              const dayTodos = todosOnDay(d);
              const items = [
                ...dayOccs.map((o) => eventChip(o)),
                ...dayTodos.map((t) => todoChip(t)),
              ];
              const extra = Math.max(0, items.length - 3);
              return (
                <div
                  key={dstr(d)}
                  className={`cal-cell${sameDay(d, today) ? " today" : ""}${d.getMonth() !== anchor.getMonth() ? " other" : ""}`}
                  title="Click to create an event on this day"
                  onClick={() =>
                    openCreateAt(
                      new Date(d.getFullYear(), d.getMonth(), d.getDate(), 9, 0),
                      new Date(d.getFullYear(), d.getMonth(), d.getDate(), 10, 0),
                    )
                  }
                >
                  <div className="cal-daynum">{d.getDate()}</div>
                  <div className="cal-cell-items">
                    {items.slice(0, 3)}
                    {extra > 0 && (
                      <button
                        className="cal-more"
                        onClick={(e) => {
                          e.stopPropagation();
                          setAnchor(d);
                          setViewMode("day");
                        }}
                      >
                        +{extra} more
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* ── Week ── */}
      {viewMode === "week" && (
        <div className="cal-week">
          <div className="cal-whead">
            <div className="cal-gutter" />
            {view.days.map((d) => (
              <div key={dstr(d)} className={`cal-whday${sameDay(d, today) ? " today" : ""}`}>
                {WEEKDAYS[(d.getDay() + 6) % 7]} <b>{d.getDate()}</b>
              </div>
            ))}
          </div>
          <div className="cal-allday">
            <div className="cal-gutter cal-adlabel">All day</div>
            {view.days.map((d) => allDayRow(d, dstr(d)))}
          </div>
          <div className="cal-scroll">
            <div className="cal-tgrid">
              <div className="cal-gutter">
                {Array.from({ length: 24 }, (_, h) => (
                  <div key={h} className="cal-hlabel" style={{ top: h * HOUR_H, height: HOUR_H }}>
                    {h === 0 ? "" : `${pad(h)}:00`}
                  </div>
                ))}
              </div>
              {view.days.map((d) => timeColumn(d, dstr(d)))}
            </div>
          </div>
        </div>
      )}

      {/* ── Day ── */}
      {viewMode === "day" && (
        <div className="cal-week cal-day">
          <div className="cal-whead">
            <div className="cal-gutter" />
            <div className={`cal-whday${sameDay(view.days[0], today) ? " today" : ""}`}>
              {view.days[0].toLocaleDateString(undefined, {
                weekday: "long",
                month: "short",
                day: "numeric",
              })}
            </div>
          </div>
          <div className="cal-allday">
            <div className="cal-gutter cal-adlabel">All day</div>
            {allDayRow(view.days[0], dstr(view.days[0]))}
          </div>
          <div className="cal-scroll">
            <div className="cal-tgrid">
              <div className="cal-gutter">
                {Array.from({ length: 24 }, (_, h) => (
                  <div key={h} className="cal-hlabel" style={{ top: h * HOUR_H, height: HOUR_H }}>
                    {h === 0 ? "" : `${pad(h)}:00`}
                  </div>
                ))}
              </div>
              {timeColumn(view.days[0], dstr(view.days[0]))}
            </div>
          </div>
        </div>
      )}

      {!loading && events.length === 0 && (
        <div className="empty-state cal-empty-hint">
          No events yet — click any date (or “+ New Event”) to create your first one.
        </div>
      )}

      {dialog && (
        <EventDialog
          state={dialog}
          onClose={() => setDialog(null)}
          onSaved={() => {
            setDialog(null);
            void refresh();
          }}
        />
      )}
    </div>
  );
}
