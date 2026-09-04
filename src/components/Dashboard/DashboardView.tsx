import { useCallback, useEffect, useState } from "react";
import { ResumeBanner } from "./ResumeBanner";
import { ContextRecall } from "./ContextRecall";
import { WellnessCard } from "./WellnessCard";
import { AlphaHunter } from "./AlphaHunter";
import { api } from "../../lib/api";
import type { planner } from "../../lib/api";

type Section = planner.BriefingSection;

/**
 * 🍓 Dashboard — the daily cockpit.
 * Briefing + charts only. Tasks/habits live in the right rail;
 * Health Lens and Export My Story are their own left-nav views.
 */
export function DashboardView() {
  const [briefing, setBriefing] = useState<Section[] | null>(null);
  const [todos, setTodos] = useState<planner.Todo[]>([]);
  const [habits, setHabits] = useState<planner.Habit[]>([]);
  const [busy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [b, t, h] = await Promise.all([
        api.getDailyBriefing(),
        api.getTodos(),
        api.getHabits(),
      ]);
      setBriefing(b);
      setTodos(t);
      setHabits(h);
      setError(null);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setBriefing([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const today = new Date().toLocaleDateString(undefined, {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  });

  return (
    <div className="content dashboard">
      <header className="dash-head">
        <div>
          <h1 className="dash-title">
            <span className="berry">🍓</span> Good to see you
          </h1>
          <div className="meta-line">{today}</div>
        </div>
        <button className="btn" onClick={() => void refresh()} disabled={busy}>
          ↻ Refresh
        </button>
      </header>

      {error && (
        <div className="dash-error">⚠️ {error}</div>
      )}

      <ResumeBanner />

      <ContextRecall />

      {/* ------------------------- New feature cards ---------------------- */}
      <section className="feature-grid" aria-label="New tools">
        <WellnessCard />
        <AlphaHunter />
      </section>

      {/* ------------------------- Briefing cards ------------------------- */}
      <section className="brief-grid" aria-label="Daily briefing">
        {briefing === null && (
          <div className="loading-block"><span className="spinner" /> Loading briefing…</div>
        )}
        {briefing?.length === 0 && !error && (
          <div className="text-dim">
            All quiet. Add a task or habit from the right rail to get started. 🌱
          </div>
        )}
        {briefing?.map((s) => (
          <article key={s.key} className={`card card-${s.key}`}>
            <h3 className="card-title">{s.title}</h3>
            <ul className="card-lines">
              {s.lines.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          </article>
        ))}
      </section>

      {/* --------------------------- Stats row ---------------------------- */}
      <section className="stats-grid" aria-label="Statistics">
        <div className="chart-card">
          <h4 className="chart-title">Today's progress</h4>
          <ProgressRing done={todos.filter((t) => t.completed).length} total={todos.length} />
        </div>
        <div className="chart-card">
          <h4 className="chart-title">Tasks by priority</h4>
          <PriorityBars todos={todos} />
        </div>
        <div className="chart-card">
          <h4 className="chart-title">Habits · last 7 days</h4>
          <HabitWeek habits={habits} />
        </div>
      </section>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Dashboard charts
//
// Pure SVG/CSS, no charting dependency. The `.ring-*`, `.bar-*` and
// `.week-dot*` classes they use already exist in global.css.
// ---------------------------------------------------------------------------

/** Completion ring for today's tasks. */
function ProgressRing({ done, total }: { done: number; total: number }) {
  const pct = total === 0 ? 0 : Math.round((done / total) * 100);
  const r = 34;
  const circumference = 2 * Math.PI * r;
  const offset = circumference * (1 - pct / 100);

  return (
    <div className="ring-wrap">
      <div className="ring">
        <svg
          width="84"
          height="84"
          viewBox="0 0 84 84"
          role="img"
          aria-label={`${pct}% of tasks complete`}
        >
          <defs>
            <linearGradient id="berryGrad" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#fb7185" />
              <stop offset="100%" stopColor="#e11d48" />
            </linearGradient>
          </defs>
          <circle className="ring-track" cx="42" cy="42" r={r} />
          <circle
            className="ring-fill"
            cx="42"
            cy="42"
            r={r}
            strokeDasharray={circumference}
            strokeDashoffset={offset}
          />
        </svg>
        <div className="ring-label" aria-hidden>
          {pct}%
          <span className="ring-sub">
            {done}/{total}
          </span>
        </div>
      </div>
      <div className="text-dim">
        {total === 0
          ? "No tasks yet."
          : done === total
            ? "All done for today. 🎉"
            : `${total - done} left to go.`}
      </div>
    </div>
  );
}

/** Open task counts per priority. */
function PriorityBars({ todos }: { todos: planner.Todo[] }) {
  const open = todos.filter((t) => !t.completed);
  const rows: { key: planner.Todo["priority"]; label: string }[] = [
    { key: "high", label: "High" },
    { key: "medium", label: "Med" },
    { key: "low", label: "Low" },
  ];
  const counts = rows.map((r) => open.filter((t) => t.priority === r.key).length);
  const max = Math.max(1, ...counts);

  if (todos.length === 0) return <div className="text-dim">No tasks yet.</div>;

  return (
    <div>
      {rows.map((row, i) => (
        <div key={row.key} className="bar-row">
          <span className="bar-label">{row.label}</span>
          <span className="bar-track">
            <span
              className={row.key === "high" ? "bar-fill is-berry" : "bar-fill"}
              style={{ width: `${(counts[i] / max) * 100}%` }}
            />
          </span>
          <span className="bar-count">{counts[i]}</span>
        </div>
      ))}
    </div>
  );
}

/** Last seven days of habit completions, oldest first. */
function HabitWeek({ habits }: { habits: planner.Habit[] }) {
  if (habits.length === 0) return <div className="text-dim">No habits yet.</div>;

  const days: string[] = [];
  for (let i = 6; i >= 0; i -= 1) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    days.push(d.toISOString().slice(0, 10));
  }

  return (
    <div>
      {habits.slice(0, 6).map((h) => {
        const done = new Set(h.completedDates);
        const hits = days.filter((d) => done.has(d)).length;
        return (
          <div key={h.id} className="bar-row">
            <span className="bar-label" title={h.name}>
              {h.icon ?? "🔥"}
            </span>
            <span
              className="week-dots"
              role="img"
              aria-label={`${h.name}: ${hits} of last 7 days`}
            >
              {days.map((d) => (
                <span
                  key={d}
                  className={done.has(d) ? "week-dot hit" : "week-dot"}
                />
              ))}
            </span>
            <span className="bar-count">{hits}</span>
          </div>
        );
      })}
    </div>
  );
}
