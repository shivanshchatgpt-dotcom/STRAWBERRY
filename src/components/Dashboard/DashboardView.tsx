import { useCallback, useEffect, useState } from "react";
import { ResumeBanner } from "./ResumeBanner";
import { ContextRecall } from "./ContextRecall";
import { api } from "../../lib/api";
import type { planner, health } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

type Section = planner.BriefingSection;

/**
 * 🍓 Dashboard — the daily cockpit.
 * Briefing on top (tasks / habits / events / memories / news),
 * quick-add row for todos + habits below.
 */
export function DashboardView() {
  const [briefing, setBriefing] = useState<Section[] | null>(null);
  const [todos, setTodos] = useState<planner.Todo[]>([]);
  const [habits, setHabits] = useState<planner.Habit[]>([]);
  const [newTodo, setNewTodo] = useState("");
  const [newPriority, setNewPriority] = useState<"low" | "medium" | "high">("medium");
  const [newHabit, setNewHabit] = useState("");
  const [busy, setBusy] = useState(false);
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

  const [health, setHealth] = useState<health.HealthReport | null>(null);
  const [healthBusy, setHealthBusy] = useState(false);

  const scanHealth = async () => {
    setHealthBusy(true);
    try {
      setHealth(await api.healthReport());
    } finally {
      setHealthBusy(false);
    }
  };

  const fmtGb = (b: number) => (b >= 1073741824 ? `${(b / 1073741824).toFixed(1)} GB` : `${Math.round(b / 1048576)} MB`);

  const [story, setStory] = useState<string | null>(null);
  const [storyBusy, setStoryBusy] = useState(false);
  const [repoPath, setRepoPath] = useState("");

  const generateStory = async () => {
    setStoryBusy(true);
    try {
      const md = await api.exportMyStory(repoPath.trim() || null, 14);
      setStory(md);
    } catch {
      setStory("Story generation failed.");
    } finally {
      setStoryBusy(false);
    }
  };

  const copyStory = async () => {
    if (story) await useAppStore.getState().copyText(story, "My Story");
  };

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const addTodo = async () => {
    const title = newTodo.trim();
    if (!title || busy) return;
    setBusy(true);
    try {
      await api.addTodo(title, newPriority, null);
      setNewTodo("");
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const addHabit = async () => {
    const name = newHabit.trim();
    if (!name || busy) return;
    setBusy(true);
    try {
      await api.addHabit(name, "🔥", null);
      setNewHabit("");
      await refresh();
    } finally {
      setBusy(false);
    }
  };

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

      {/* ------------------------- Briefing cards ------------------------- */}
      <section className="brief-grid" aria-label="Daily briefing">
        {briefing === null && (
          <div className="loading-block"><span className="spinner" /> Loading briefing…</div>
        )}
        {briefing?.length === 0 && !error && (
          <div className="text-dim">
            All quiet. Add a task or habit below to get started. 🌱
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

      {/* ----------------------------- Todos ------------------------------ */}
      <section className="panel" aria-label="Tasks">
        <h3 className="panel-title">📋 Tasks</h3>
        <div className="quick-row">
          <input
            className="quick-input"
            placeholder="Add a task…"
            value={newTodo}
            onChange={(e) => setNewTodo(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addTodo()}
          />
          <select
            className="quick-select"
            value={newPriority}
            onChange={(e) => setNewPriority(e.target.value as typeof newPriority)}
            aria-label="Priority"
          >
            <option value="high">🔴 High</option>
            <option value="medium">🟡 Medium</option>
            <option value="low">🟢 Low</option>
          </select>
          <button className="btn primary" onClick={() => void addTodo()} disabled={busy}>
            Add
          </button>
        </div>
        <ul className="todo-list">
          {todos.map((t) => (
            <li key={t.id} className={t.completed ? "done" : ""}>
              <label className="todo-row">
                <input
                  type="checkbox"
                  checked={t.completed}
                  onChange={() =>
                    void api.toggleTodo(t.id).then(refresh)
                  }
                />
                <span className={`prio prio-${t.priority}`} aria-hidden />
                <span className="todo-title">{t.title}</span>
                {t.dueDate && <time className="todo-due">{t.dueDate.slice(0, 10)}</time>}
                <button
                  className="icon-btn"
                  title="Delete"
                  onClick={() => void api.deleteTodo(t.id).then(refresh)}
                >
                  ✕
                </button>
              </label>
            </li>
          ))}
          {todos.length === 0 && <li className="text-dim">No tasks yet.</li>}
        </ul>
      </section>

      {/* ---------------------------- Habits ------------------------------ */}
      <section className="panel" aria-label="Habits">
        <h3 className="panel-title">🔥 Habits</h3>
        <div className="quick-row">
          <input
            className="quick-input"
            placeholder="New habit (e.g. Read 20 min)…"
            value={newHabit}
            onChange={(e) => setNewHabit(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addHabit()}
          />
          <button className="btn primary" onClick={() => void addHabit()} disabled={busy}>
            Add
          </button>
        </div>
        <div className="habit-grid">
          {habits.map((h) => {
            const todayStr = new Date().toISOString().slice(0, 10);
            const doneToday = h.completedDates.includes(todayStr);
            let streak = 0;
            const setD = new Set(h.completedDates);
            const d = new Date();
            if (!setD.has(d.toISOString().slice(0, 10))) d.setDate(d.getDate() - 1);
            while (setD.has(d.toISOString().slice(0, 10))) {
              streak += 1;
              d.setDate(d.getDate() - 1);
            }
            return (
              <button
                key={h.id}
                className={`habit-chip${doneToday ? " done" : ""}`}
                onClick={() => void api.toggleHabitToday(h.id).then(refresh)}
                title={doneToday ? "Done today — click to undo" : "Mark done for today"}
              >
                <span className="habit-icon">{h.icon ?? "🔥"}</span>
                <span className="habit-name">{h.name}</span>
                <span className="habit-streak">{streak}d</span>
              </button>
            );
          })}
          {habits.length === 0 && <div className="text-dim">No habits yet.</div>}
        </div>
      </section>

      {/* ---------------------------- Health Lens -------------------------- */}
      <section className="panel" aria-label="Health Lens">
        <h3 className="panel-title">🩺 Health Lens</h3>
        <p className="text-dim" style={{ fontSize: 12.5, margin: "4px 0 10px" }}>
          Read-only scan: disk space, cache bloat, biggest home folders. Kuch bhi delete nahi hota.
        </p>
        <div className="quick-row">
          <button className="btn primary" disabled={healthBusy} onClick={() => void scanHealth()}>
            {healthBusy ? <span className="spinner" /> : null} 🩺 Scan Now
          </button>
        </div>
        {health && (
          <div className="health-grid">
            <div className="stat-box">
              <div className="stat-value">{fmtGb(health.diskFreeBytes)}</div>
              <div className="stat-label">Disk free</div>
            </div>
            {health.caches.slice(0, 4).map((c) => (
              <div key={c.path} className="stat-box">
                <div className="stat-value">{fmtGb(c.bytes)}</div>
                <div className="stat-label">{c.path}</div>
              </div>
            ))}
          </div>
        )}
        {health && (
          <ul className="text-dim" style={{ fontSize: 12.5, marginTop: 10 }}>
            {health.notes.map((n, i) => (
              <li key={i}>{n}</li>
            ))}
            {health.topHomeDirs.length > 0 && (
              <li>
                Biggest home folders:{" "}
                {health.topHomeDirs
                  .map((d) => `${d.path} (${fmtGb(d.bytes)})`)
                  .join(" · ")}
              </li>
            )}
          </ul>
        )}
      </section>

      {/* ---------------------------- Export My Story --------------------- */}
      <section className="panel" aria-label="Export My Story">
        <h3 className="panel-title">📖 Export My Story</h3>
        <p className="text-dim" style={{ fontSize: 12.5, margin: "4px 0 10px" }}>
          Buildathon-ready narrative — chats, captures, tasks, habits (+ optional
          git commit timeline). 100% local, zero LLM.
        </p>
        <div className="quick-row">
          <input
            className="quick-input"
            placeholder="Optional: path to a git repo for commit timeline…"
            value={repoPath}
            onChange={(e) => setRepoPath(e.target.value)}
          />
          <button
            className="btn primary"
            disabled={storyBusy}
            onClick={() => void generateStory()}
          >
            {storyBusy ? <span className="spinner" /> : null} Generate
          </button>
          {story && (
            <button className="btn" onClick={() => void copyStory()}>
              📋 Copy
            </button>
          )}
        </div>
        {story && <pre className="pre-block brief-text story-output">{story}</pre>}
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
