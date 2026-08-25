import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { planner } from "../../lib/api";

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
    </div>
  );
}
