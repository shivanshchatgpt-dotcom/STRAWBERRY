import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../lib/api";
import type { planner } from "../../lib/api";
import { useAppStore } from "../../store/appStore";
import { PreviousWorkPanel } from "./PreviousWorkPanel";

/**
 * 🗓️ Planner — the anime-planner feature set merged into Strawberry.
 * Habits (streaks, consistency rings, backfill, month calendar),
 * Focus (timer presets + stopwatch + stats) and Schedule (day / next
 * 48h / week / month calendar).
 */

type Panel = "habits" | "focus" | "schedule" | "previous_work";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function dstr(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
function addDays(d: Date, n: number): Date {
  const x = new Date(d);
  x.setDate(x.getDate() + n);
  return x;
}
function last7(): string[] {
  const out: string[] = [];
  for (let i = 6; i >= 0; i--) out.push(dstr(addDays(new Date(), -i)));
  return out;
}
function currentStreak(set: Set<string>): number {
  let n = 0;
  const t = new Date();
  if (!set.has(dstr(t))) t.setDate(t.getDate() - 1);
  while (set.has(dstr(t))) {
    n++;
    t.setDate(t.getDate() - 1);
  }
  return n;
}
function bestStreak(dates: string[]): number {
  const sorted = [...new Set(dates)].sort();
  let best = 0;
  let run = 0;
  let prev: Date | null = null;
  for (const s of sorted) {
    const d = new Date(s + "T00:00:00");
    run = prev && d.getTime() - prev.getTime() === 86400000 ? run + 1 : 1;
    best = Math.max(best, run);
    prev = d;
  }
  return best;
}
function consistency30(dates: string[]): number {
  const set = new Set(dates);
  let hit = 0;
  for (let i = 0; i < 30; i++) if (set.has(dstr(addDays(new Date(), -i)))) hit++;
  return Math.round((hit / 30) * 100);
}

export function PlannerView() {
  const [panel, setPanel] = useState<Panel>("habits");

  return (
    <div className="content planner">
      <header className="page-head">
        <div>
          <h1 className="dash-title">🗓️ Planner</h1>
          <div className="meta-line">Habits · Focus · Schedule — pura planner ek jagah</div>
        </div>
        <nav className="nav-tabs">
          {(
            [
              ["habits", "🔥 Habits"],
              ["focus", "⏱️ Focus"],
              ["schedule", "📅 Schedule"],
              ["previous_work", "⏮️ Previous Work"],
            ] as [Panel, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              className={`nav-tab${panel === key ? " active" : ""}`}
              onClick={() => setPanel(key)}
            >
              {label}
            </button>
          ))}
        </nav>
      </header>

      {panel === "habits" && <HabitsPanel />}
      {panel === "focus" && <FocusPanel />}
      {panel === "schedule" && <SchedulePanel />}
      {panel === "previous_work" && <PreviousWorkPanel />}
    </div>
  );
}

/* ══════════════════════════ HABITS ══════════════════════════ */

function HabitsPanel() {
  const [habits, setHabits] = useState<planner.Habit[]>([]);
  const [calFor, setCalFor] = useState<number | null>(null);
  const showToast = useAppStore((s) => s.showToast);

  const refresh = useCallback(async () => {
    try {
      setHabits(await api.getHabits());
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  }, [showToast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const tickDate = async (habitId: number, date: string) => {
    try {
      await api.toggleHabitDate(habitId, date);
      await refresh();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const tickToday = async (habitId: number) => tickDate(habitId, dstr(new Date()));

  const week = last7();
  const todayStr = dstr(new Date());
  const doneToday = habits.filter((h) => new Set(h.completedDates).has(todayStr)).length;

  // month calendar for expanded habit
  const now = new Date();
  const firstDow = (new Date(now.getFullYear(), now.getMonth(), 1).getDay() + 6) % 9 % 7;
  const daysInMonth = new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();

  return (
    <div className="planner-body">
      <section className="panel pad habit-summary">
        <div className="habit-summary-head">
          <div>
            <div className="section-label">Today's Check-ins</div>
            <p className="meta-line">
              {habits.length === 0
                ? "Neeche se pehli habit banao"
                : doneToday === habits.length
                  ? "Perfect day — sab done! 🎉"
                  : `${doneToday} of ${habits.length} habits done`}
            </p>
          </div>
        </div>
        {habits.length > 0 && (
          <div className="habit-bars">
            {habits.map((h) => (
              <div key={h.id} className="habit-bar">
                <span style={{ background: h.color ?? "#fb7185", width: new Set(h.completedDates).has(todayStr) ? "100%" : "0%" }} />
              </div>
            ))}
          </div>
        )}
      </section>

      {habits.length === 0 && (
        <div className="panel pad text-dim">
          Dashboard ke quick-add se habit banao — yahan full stats milenge.
        </div>
      )}

      {habits.map((h) => {
        const set = new Set(h.completedDates);
        const streak = currentStreak(set);
        const best = Math.max(bestStreak(h.completedDates), streak);
        const cons = consistency30(h.completedDates);
        const CIRC = 113;
        return (
          <section className="panel pad habit-card" key={h.id}>
            <div className="habit-row">
              <button
                className={`tick big${set.has(todayStr) ? " on" : ""}`}
                style={
                  set.has(todayStr)
                    ? { background: `linear-gradient(135deg, ${h.color ?? "#34d399"}, ${h.color ?? "#34d399"}cc)`, boxShadow: `0 0 18px ${h.color ?? "#34d399"}55` }
                    : undefined
                }
                onClick={() => void tickToday(h.id)}
                title={set.has(todayStr) ? "Done! Click to undo" : "Mark as done"}
              >
                {set.has(todayStr) ? "✓" : h.icon || "✨"}
              </button>

              <div className="habit-info">
                <div className="habit-name-row">
                  <strong>{h.name}</strong>
                  {streak > 0 && <span className="chip-streak">🔥 {streak}d</span>}
                  {best > 1 && <span className="chip-best">best {best}</span>}
                </div>
                {h.description && <p className="habit-desc">{h.description}</p>}
                <div className="habit-meta">
                  <span>{h.completedDates.length} total</span>
                  <span>·</span>
                  <span>{cons}% last 30d</span>
                  <span>·</span>
                  <span>target {h.targetDays}d</span>
                </div>
              </div>

              <svg width="46" height="46" viewBox="0 0 44 44" className="cons-ring" aria-hidden>
                <circle cx="22" cy="22" r="18" fill="none" stroke="rgba(148,163,184,0.18)" strokeWidth="4" />
                <circle
                  cx="22" cy="22" r="18" fill="none"
                  stroke={h.color ?? "#34d399"} strokeWidth="4" strokeLinecap="round"
                  strokeDasharray={CIRC} strokeDashoffset={CIRC * (1 - cons / 100)}
                  transform="rotate(-90 22 22)"
                />
                <text x="22" y="26" textAnchor="middle" fontSize="10" fill="currentColor">{cons}%</text>
              </svg>
            </div>

            {/* clickable week strip — backfill any day */}
            <div className="week-strip">
              {week.map((ds) => {
                const dt = new Date(ds + "T00:00:00");
                const isToday = ds === todayStr;
                const done = set.has(ds);
                return (
                  <button
                    key={ds}
                    className={`week-day${done ? " done" : ""}${isToday ? " today" : ""}`}
                    style={done ? { background: `${h.color ?? "#34d399"}18`, borderColor: `${h.color ?? "#34d399"}55` } : undefined}
                    onClick={() => void tickDate(h.id, ds)}
                    title={`${ds} — ${done ? "done (click to undo)" : "not done (click to mark)"}`}
                  >
                    <span>{WEEKDAYS[(dt.getDay() + 6) % 7].slice(0, 2)}</span>
                    <b>{dt.getDate()}</b>
                    <em>{done ? "✓" : ""}</em>
                  </button>
                );
              })}
              <button className="btn small cal-toggle" onClick={() => setCalFor(calFor === h.id ? null : h.id)}>
                🗓️ Month
              </button>
            </div>

            {calFor === h.id && (
              <div className="month-cal">
                <div className="month-cal-head">{now.toLocaleString(undefined, { month: "long", year: "numeric" })}</div>
                <div className="cal-grid">
                  {WEEKDAYS.map((w) => <span key={w} className="cal-dow">{w.slice(0, 1)}</span>)}
                  {Array.from({ length: firstDow }).map((_, i) => <span key={`e${i}`} />)}
                  {Array.from({ length: daysInMonth }).map((_, i) => {
                    const ds = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(i + 1).padStart(2, "0")}`;
                    const done = set.has(ds);
                    return (
                      <button
                        key={ds}
                        className={`cal-cell${done ? " done" : ""}${ds === todayStr ? " today" : ""}`}
                        style={done ? { background: h.color ?? "#34d399", borderColor: h.color ?? "#34d399" } : undefined}
                        onClick={() => void tickDate(h.id, ds)}
                      >
                        {i + 1}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}

/* ══════════════════════════ FOCUS ══════════════════════════ */

const PRESETS: { label: string; minutes: number }[] = [
  { label: "Sprint", minutes: 15 },
  { label: "Focus", minutes: 25 },
  { label: "Deep Work", minutes: 50 },
  { label: "Break", minutes: 5 },
];

const RING = 220;
const STROKE = 13;
const RR = (RING - STROKE) / 2;
const CIRC = 2 * Math.PI * RR;

function fmtClock(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function FocusPanel() {
  const [mode, setMode] = useState<"timer" | "stopwatch">("timer");
  const [preset, setPreset] = useState(PRESETS[1]);
  const [customMin, setCustomMin] = useState("");
  const [secondsLeft, setSecondsLeft] = useState(PRESETS[1].minutes * 60);
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);
  const endTimeRef = useRef<number | null>(null);

  // stopwatch
  const [elapsed, setElapsed] = useState(0);
  const swStartRef = useRef<number | null>(null);

  const [stats, setStats] = useState<planner.FocusStats | null>(null);
  const [label, setLabel] = useState("");
  const showToast = useAppStore((s) => s.showToast);

  const refreshStats = useCallback(async () => {
    try {
      setStats(await api.getFocusStats());
    } catch {
      /* silent */
    }
  }, []);

  useEffect(() => {
    void refreshStats();
  }, [refreshStats]);

  useEffect(() => {
    if (!running || mode !== "timer") return;
    const tick = () => {
      const left = Math.max(0, Math.round(((endTimeRef.current ?? 0) - Date.now()) / 1000));
      setSecondsLeft(left);
      if (left <= 0) {
        setRunning(false);
        endTimeRef.current = null;
        if (preset.label !== "Break") {
          void api.logFocusSession(preset.minutes, preset.label, "timer").then(() => {
            setDone(true);
            void refreshStats();
            showToast("success", `🎉 ${preset.label} complete — ${preset.minutes}m logged`);
            setTimeout(() => setDone(false), 6000);
          });
        }
      }
    };
    tick();
    const iv = setInterval(tick, 500);
    return () => clearInterval(iv);
  }, [running, mode, preset, refreshStats, showToast]);

  useEffect(() => {
    if (!running || mode !== "stopwatch") return;
    const iv = setInterval(() => {
      setElapsed(Math.round((Date.now() - (swStartRef.current ?? Date.now())) / 1000));
    }, 500);
    return () => clearInterval(iv);
  }, [running, mode]);

  const pickPreset = (p: typeof PRESETS[number]) => {
    setPreset(p);
    setRunning(false);
    endTimeRef.current = null;
    setSecondsLeft(p.minutes * 60);
  };
  const applyCustom = () => {
    const m = parseInt(customMin, 10);
    if (!Number.isFinite(m) || m <= 0 || m > 600) return;
    pickPreset({ label: `Custom ${m}m`, minutes: m });
  };

  const startTimer = () => {
    endTimeRef.current = Date.now() + secondsLeft * 1000;
    setRunning(true);
  };

  const startStopwatch = () => {
    swStartRef.current = Date.now() - elapsed * 1000;
    setRunning(true);
  };
  const stopStopwatch = async () => {
    setRunning(false);
    const mins = Math.max(1, Math.round(elapsed / 60));
    try {
      await api.logFocusSession(mins, label.trim() || null, "stopwatch");
      showToast("success", `⏱️ ${mins}m logged`);
      setElapsed(0);
      swStartRef.current = null;
      await refreshStats();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const total = preset.minutes * 60;
  const pct = ((total - secondsLeft) / total) * 100;
  const isBreak = preset.label === "Break";
  const accent = isBreak ? "#38bdf8" : "#f43f5e";

  return (
    <div className="planner-body focus-wrap">
      <nav className="nav-tabs self-center">
        <button className={`nav-tab${mode === "timer" ? " active" : ""}`} onClick={() => { setMode("timer"); setRunning(false); }}>
          ⏳ Timer
        </button>
        <button className={`nav-tab${mode === "stopwatch" ? " active" : ""}`} onClick={() => { setMode("stopwatch"); setRunning(false); }}>
          ⏱️ Stopwatch
        </button>
      </nav>

      <section className="panel pad focus-card">
        {mode === "timer" ? (
          <>
            <div className="preset-row">
              {PRESETS.map((p) => (
                <button key={p.label} className={`btn small${preset.label === p.label ? " primary" : ""}`} disabled={running} onClick={() => pickPreset(p)}>
                  {p.label} · {p.minutes}m
                </button>
              ))}
              <input
                className="focus-custom"
                placeholder="custom m"
                inputMode="numeric"
                value={customMin}
                onChange={(e) => setCustomMin(e.target.value.replace(/\D/g, ""))}
                onKeyDown={(e) => e.key === "Enter" && applyCustom()}
                onBlur={applyCustom}
              />
            </div>

            <div className="ring-holder" style={{ width: RING, height: RING }}>
              <svg width={RING} height={RING} className="ring-svg">
                <circle cx={RING / 2} cy={RING / 2} r={RR} fill="none" stroke="rgba(148,163,184,0.15)" strokeWidth={STROKE} />
                <circle
                  cx={RING / 2} cy={RING / 2} r={RR} fill="none"
                  stroke={accent} strokeWidth={STROKE} strokeLinecap="round"
                  strokeDasharray={CIRC} strokeDashoffset={CIRC * (1 - pct / 100)}
                  transform={`rotate(-90 ${RING / 2} ${RING / 2})`}
                  style={{ transition: "stroke-dashoffset 0.5s linear", filter: running ? `drop-shadow(0 0 14px ${accent}88)` : "none" }}
                />
              </svg>
              <div className="ring-center">
                {done ? (
                  <>
                    <span className="big-emoji">🎉</span>
                    <b>Session logged!</b>
                  </>
                ) : (
                  <>
                    <span className="focus-clock">{fmtClock(secondsLeft)}</span>
                    <small>{running ? (isBreak ? "recharging" : "in the zone") : preset.label}</small>
                  </>
                )}
              </div>
            </div>

            <div className="focus-actions">
              {!running ? (
                <button className="btn primary big-btn" disabled={secondsLeft === 0} onClick={startTimer}>
                  ▶ {secondsLeft === total ? "Start" : "Resume"}
                </button>
              ) : (
                <button className="btn big-btn" onClick={() => { setRunning(false); endTimeRef.current = null; }}>
                  ⏸ Pause
                </button>
              )}
              <button className="btn" onClick={() => pickPreset(preset)}>↺ Reset</button>
            </div>
          </>
        ) : (
          <>
            <div className="ring-holder" style={{ width: RING, height: RING }}>
              <div className="ring-center col">
                <span className="focus-clock">{fmtClock(elapsed)}</span>
                <small>{running ? "counting…" : "free run — stop to log"}</small>
              </div>
            </div>
            <input
              className="focus-label"
              placeholder="kis cheez pe focus? (optional)"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
            />
            <div className="focus-actions">
              {!running ? (
                <button className="btn primary big-btn" onClick={startStopwatch}>▶ Start</button>
              ) : (
                <button className="btn danger big-btn" onClick={() => void stopStopwatch()}>■ Stop & Log</button>
              )}
              {elapsed > 0 && !running && (
                <button className="btn" onClick={() => setElapsed(0)}>↺ Reset</button>
              )}
            </div>
          </>
        )}
      </section>

      <section className="stat-grid">
        <div className="panel pad stat-box"><b className="berry-text">{stats?.todaySessions ?? 0}</b><span>sessions today</span></div>
        <div className="panel pad stat-box"><b className="sky-text">{stats?.todayMinutes ?? 0}m</b><span>focus today</span></div>
        <div className="panel pad stat-box"><b className="violet-text">{Math.round(((stats?.totalMinutes ?? 0) / 60) * 10) / 10}h</b><span>all time</span></div>
      </section>

      {stats && stats.recent.length > 0 && (
        <section className="panel pad">
          <div className="section-label">Recent Sessions</div>
          <ul className="session-list">
            {stats.recent.map((s) => (
              <li key={s.id}>
                <span>{s.kind === "stopwatch" ? "⏱️" : "⏳"} {s.label || "Focus"}</span>
                <span className="mono dim">{s.minutes}m · {s.completedAt.slice(5, 16)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/* ══════════════════════════ SCHEDULE & CALENDAR ══════════════════════════ */

type SchedView = "day" | "48h" | "week" | "month";

const CATEGORY_COLORS: Record<string, string> = {
  workshop: "#f43f5e",
  hackathon: "#8b5cf6",
  focus: "#38bdf8",
  general: "#34d399",
  exam: "#f59e0b",
};

function SchedulePanel() {
  const [view, setView] = useState<SchedView>("week");
  const [events, setEvents] = useState<import("../../lib/types").CalendarEvent[]>([]);
  const [showModal, setShowModal] = useState(false);
  const [title, setTitle] = useState("");
  const [category, setCategory] = useState("workshop");
  const [startAt, setStartAt] = useState("");
  const [endAt, setEndAt] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [location, setLocation] = useState("");
  const [certificate, setCertificate] = useState(false);
  const [selDay, setSelDay] = useState<string>(dstr(new Date()));

  const showToast = useAppStore((s) => s.showToast);

  const refresh = useCallback(async () => {
    try {
      setEvents(await api.listCalendarEvents());
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  }, [showToast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const addEvent = async () => {
    if (!title.trim() || !startAt || !endAt) return;
    try {
      await api.createCalendarEvent({
        title: title.trim(),
        category,
        startAt: new Date(startAt).toISOString(),
        endAt: new Date(endAt).toISOString(),
        sourceUrl: sourceUrl.trim() || undefined,
        location: location.trim() || undefined,
        certificateOffered: certificate,
        registrationRequired: !!sourceUrl.trim(),
        reminderMinutes: [15, 60],
      });
      setTitle("");
      setStartAt("");
      setEndAt("");
      setSourceUrl("");
      setLocation("");
      setCertificate(false);
      setShowModal(false);
      await refresh();
      showToast("success", "📅 Event & local reminders created!");
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const deleteEv = async (id: string) => {
    try {
      await api.deleteCalendarEvent(id);
      await refresh();
      showToast("success", "Deleted event");
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const range: Date[] = useMemo(() => {
    const today = new Date();
    if (view === "day") return [today];
    if (view === "48h") return [today, addDays(today, 1)];
    if (view === "week") return Array.from({ length: 7 }, (_, i) => addDays(today, i));
    const y = today.getFullYear();
    const m = today.getMonth();
    const n = new Date(y, m + 1, 0).getDate();
    return Array.from({ length: n }, (_, i) => new Date(y, m, i + 1));
  }, [view]);

  const byDay = useMemo(() => {
    const map = new Map<string, import("../../lib/types").CalendarEvent[]>();
    for (const ev of events) {
      const k = ev.startAt.slice(0, 10);
      (map.get(k) ?? map.set(k, []).get(k)!).push(ev);
    }
    return map;
  }, [events]);

  const dayLabel = (d: Date) =>
    dstr(d) === dstr(new Date()) ? "Today (Aaj)" : dstr(d) === dstr(addDays(new Date(), 1)) ? "Tomorrow (Kal)" :
      d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });

  const now = new Date();
  const firstDow = (new Date(now.getFullYear(), now.getMonth(), 1).getDay() + 6) % 9 % 7;

  return (
    <div className="planner-body">
      <div className="sched-toolbar" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <nav className="nav-tabs">
          {([["day", "Day"], ["48h", "Next 48 Hours"], ["week", "Week"], ["month", "Month Calendar"]] as [SchedView, string][]).map(([k, l]) => (
            <button key={k} className={`nav-tab${view === k ? " active" : ""}`} onClick={() => setView(k)}>{l}</button>
          ))}
        </nav>
        <button className="btn primary" onClick={() => setShowModal(true)}>+ Schedule Event / Workshop</button>
      </div>

      {showModal && (
        <div className="modal-backdrop" style={{ background: "rgba(0,0,0,0.6)", position: "fixed", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", zIndex: 1000 }}>
          <div className="panel pad modal-box" style={{ width: 440, background: "var(--bg-panel)", borderRadius: 12, border: "1px solid var(--border)" }}>
            <h3 style={{ margin: "0 0 12px 0" }}>📅 Schedule Event & Reminder</h3>
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              <input placeholder="Workshop / Event Title" value={title} onChange={(e) => setTitle(e.target.value)} />
              <div style={{ display: "flex", gap: 8 }}>
                <select value={category} onChange={(e) => setCategory(e.target.value)} style={{ flex: 1, padding: "8px", borderRadius: 6 }}>
                  <option value="workshop">🎓 Workshop</option>
                  <option value="hackathon">⚡ Hackathon</option>
                  <option value="focus">⏱️ Focus Session</option>
                  <option value="exam">📝 Exam / Deadline</option>
                  <option value="general">📅 General</option>
                </select>
                <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 13, cursor: "pointer" }}>
                  <input type="checkbox" checked={certificate} onChange={(e) => setCertificate(e.target.checked)} />
                  🎓 Certificate
                </label>
              </div>
              <div style={{ display: "flex", gap: 8 }}>
                <input type="datetime-local" value={startAt} onChange={(e) => setStartAt(e.target.value)} style={{ flex: 1 }} />
                <input type="datetime-local" value={endAt} onChange={(e) => setEndAt(e.target.value)} style={{ flex: 1 }} />
              </div>
              <input placeholder="Registration URL (e.g. https://workshop.dev)" value={sourceUrl} onChange={(e) => setSourceUrl(e.target.value)} />
              <input placeholder="Location or Online Room" value={location} onChange={(e) => setLocation(e.target.value)} />
              <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 12 }}>
                <button className="btn" onClick={() => setShowModal(false)}>Cancel</button>
                <button className="btn primary" onClick={() => void addEvent()} disabled={!title.trim() || !startAt || !endAt}>Save & Remind</button>
              </div>
            </div>
          </div>
        </div>
      )}

      {view === "month" ? (
        <section className="panel pad">
          <div className="month-cal-head">{range[0].toLocaleString(undefined, { month: "long", year: "numeric" })}</div>
          <div className="cal-grid cal-lg">
            {WEEKDAYS.map((w) => <span key={w} className="cal-dow">{w}</span>)}
            {Array.from({ length: firstDow }).map((_, i) => <span key={`e${i}`} />)}
            {Array.from({ length: range.length }).map((_, i) => {
              const d = range[i];
              const ds = dstr(d);
              const dayEvs = byDay.get(ds) ?? [];
              return (
                <button
                  key={ds}
                  className={`cal-cell cal-day-cell${ds === selDay ? " selected" : ""}${ds === dstr(new Date()) ? " today" : ""}`}
                  onClick={() => setSelDay(ds)}
                >
                  <b>{d.getDate()}</b>
                  <span className="dots">{dayEvs.slice(0, 4).map((ev, j) => <i key={j} style={{ background: CATEGORY_COLORS[ev.category] ?? "#34d399" }} />)}</span>
                </button>
              );
            })}
          </div>
          {(byDay.get(selDay) ?? []).length > 0 && (
            <div className="agenda" style={{ marginTop: 16 }}>
              <div className="section-label">{selDay} Events</div>
              {(byDay.get(selDay) ?? []).map((ev) => (
                <CalendarRow key={ev.id} ev={ev} onDelete={() => void deleteEv(ev.id)} />
              ))}
            </div>
          )}
        </section>
      ) : (
        <section className="panel pad agenda">
          {range.map((d) => {
            const ds = dstr(d);
            const list = byDay.get(ds) ?? [];
            return (
              <div key={ds} className="day-block" style={{ marginBottom: 16 }}>
                <h4 className="day-title" style={{ borderBottom: "1px solid var(--border)", paddingBottom: 4 }}>{dayLabel(d)} ({ds})</h4>
                {list.length === 0 ? (
                  <p className="text-dim" style={{ fontSize: 13, margin: "6px 0" }}>No events scheduled.</p>
                ) : (
                  list.map((ev) => <CalendarRow key={ev.id} ev={ev} onDelete={() => void deleteEv(ev.id)} />)
                )}
              </div>
            );
          })}
        </section>
      )}
    </div>
  );
}

function CalendarRow({ ev, onDelete }: { ev: import("../../lib/types").CalendarEvent; onDelete: () => void }) {
  const color = CATEGORY_COLORS[ev.category] ?? "#34d399";
  const start = new Date(ev.startAt);
  const timeStr = isNaN(start.getTime()) ? ev.startAt : start.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  return (
    <div className="agenda-row" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "8px 12px", background: "rgba(255,255,255,0.03)", borderRadius: 8, margin: "4px 0" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span className="agenda-dot" style={{ width: 10, height: 10, borderRadius: "50%", background: color }} />
        <div>
          <div style={{ fontWeight: 600, display: "flex", alignItems: "center", gap: 6 }}>
            {ev.title}
            {ev.certificateOffered && <span style={{ fontSize: 11, background: "rgba(244,63,94,0.15)", color: "#f43f5e", padding: "2px 6px", borderRadius: 4 }}>🎓 Certificate</span>}
            {ev.category && <span style={{ fontSize: 11, background: "rgba(255,255,255,0.08)", textTransform: "capitalize", padding: "2px 6px", borderRadius: 4 }}>{ev.category}</span>}
          </div>
          {ev.sourceUrl && (
            <a href={ev.sourceUrl} target="_blank" rel="noreferrer" style={{ fontSize: 12, color: "var(--accent)", textDecoration: "none" }}>
              🔗 Review &amp; Register
            </a>
          )}
        </div>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <span className="mono dim" style={{ fontSize: 13 }}>{timeStr}</span>
        <button className="btn danger small" onClick={onDelete} style={{ padding: "2px 6px" }}>✕</button>
      </div>
    </div>
  );
}


