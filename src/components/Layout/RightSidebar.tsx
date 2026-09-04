import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import type { planner } from "../../lib/api";
import { ChartsRailPanel } from "./ChartsRailPanel";

/**
 * 🍓 Right rail — real working widgets, no mock data.
 *
 *   ┌─ Habits panel ─────────┐
 *   │ • get_habits           │
 *   │ • toggle_habit_today   │
 *   │ • add_habit            │
 *   ├─ Today's tasks panel ──┤
 *   │ • get_todos            │
 *   │ • toggle / delete todo │
 *   │ • add quick todo       │
 *   └────────────────────────┘
 */

const ICONS = ["🧠", "🥣", "🏃", "📖", "✍️", "💻", "🍎", "💧", "🧘", "🛌", "🎯", "🎵", "🪥", "🌱", "✏️"];

function pickIcon(name: string, stored: string | null): string {
  if (stored) return stored;
  const n = name.toLowerCase();
  if (n.includes("run") || n.includes("walk") || n.includes("jog")) return "🏃";
  if (n.includes("read") || n.includes("book")) return "📖";
  if (n.includes("write") || n.includes("journal")) return "✍️";
  if (n.includes("code") || n.includes("program")) return "💻";
  if (n.includes("protein") || n.includes("bowl") || n.includes("food") || n.includes("eat")) return "🥣";
  if (n.includes("medit") || n.includes("mind") || n.includes("yoga")) return "🧘";
  if (n.includes("water") || n.includes("drink")) return "💧";
  if (n.includes("sleep") || n.includes("bed")) return "🛌";
  if (n.includes("music") || n.includes("play")) return "🎵";
  if (n.includes("brush") || n.includes("teeth")) return "🪥";
  return ICONS[Math.abs(name.length) % ICONS.length];
}

const todayStr = () => new Date().toISOString().slice(0, 10);

function computeStreak(dates: string[]): number {
  if (!dates || dates.length === 0) return 0;
  const set = new Set(dates);
  const d = new Date();
  if (!set.has(d.toISOString().slice(0, 10))) d.setDate(d.getDate() - 1);
  let n = 0;
  while (set.has(d.toISOString().slice(0, 10))) {
    n += 1;
    d.setDate(d.getDate() - 1);
  }
  return n;
}

const prioColor: Record<string, string> = {
  high: "#ef4444",
  medium: "#f59e0b",
  low: "#3b82f6",
};

function HabitPanel() {
  const [habits, setHabits] = useState<planner.Habit[]>([]);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [icon, setIcon] = useState("🔥");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setHabits(await api.getHabits());
    } catch {
      /* silent */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 30_000);
    return () => clearInterval(t);
  }, [refresh]);

  const add = async () => {
    const title = name.trim();
    if (!title || busy) return;
    setBusy(true);
    try {
      await api.addHabit(title, icon || null, null);
      setName("");
      setAdding(false);
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (id: number) => {
    try {
      await api.toggleHabitToday(id);
      await refresh();
    } catch {
      /* silent */
    }
  };

  const t = todayStr();
  const doneCount = habits.filter((h) => h.completedDates.includes(t)).length;

  return (
    <section className="rail-panel">
      <header className="rail-panel-head">
        <div>
          <h4 className="rail-panel-title">🔥 Habits</h4>
          <span className="rail-panel-sub">Today</span>
        </div>
        <span className="rail-panel-count">{doneCount}/{habits.length}</span>
      </header>

      <ul className="habit-rail-list">
        {habits.map((h) => {
          const done = h.completedDates.includes(t);
          const streak = computeStreak(h.completedDates);
          return (
            <li key={h.id}>
              <button
                className={`habit-rail-row${done ? " done" : ""}`}
                onClick={() => void toggle(h.id)}
                title={done ? "Done today — click to undo" : "Mark done today"}
              >
                <span className="habit-rail-icon" aria-hidden>
                  {pickIcon(h.name, h.icon)}
                </span>
                <span className="habit-rail-name">{h.name}</span>
                {streak > 0 && (
                  <span className="habit-rail-streak" title={`${streak}-day streak`}>
                    {streak}d
                  </span>
                )}
              </button>
            </li>
          );
        })}
        {habits.length === 0 && (
          <li className="text-dim rail-panel-empty">No habits yet. Add one below.</li>
        )}
      </ul>

      {adding ? (
        <div className="habit-rail-add">
          <div className="habit-rail-icon-pick" role="radiogroup" aria-label="Pick an icon">
            {ICONS.map((ic) => (
              <button
                key={ic}
                type="button"
                className={`habit-rail-icon-opt${icon === ic ? " active" : ""}`}
                onClick={() => setIcon(ic)}
                aria-checked={icon === ic}
                role="radio"
              >
                {ic}
              </button>
            ))}
          </div>
          <div className="habit-rail-add-row">
            <input
              className="quick-input"
              placeholder="Habit name…"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void add()}
              autoFocus
            />
            <button className="btn primary" disabled={busy} onClick={() => void add()}>+</button>
            <button className="btn" onClick={() => { setAdding(false); setName(""); }}>×</button>
          </div>
        </div>
      ) : (
        <button className="habit-rail-add-cta" onClick={() => setAdding(true)}>
          <span aria-hidden>＋</span> Add new habit
        </button>
      )}
    </section>
  );
}

function TasksPanel() {
  const [todos, setTodos] = useState<planner.Todo[]>([]);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setTodos(await api.getTodos());
    } catch {
      /* silent */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 30_000);
    return () => clearInterval(t);
  }, [refresh]);

  const add = async () => {
    const title = text.trim();
    if (!title || busy) return;
    setBusy(true);
    try {
      await api.addTodo(title, "medium", null);
      setText("");
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (id: number) => {
    try { await api.toggleTodo(id); await refresh(); } catch { /* silent */ }
  };

  const remove = async (id: number) => {
    try { await api.deleteTodo(id); await refresh(); } catch { /* silent */ }
  };

  const open = useMemo(() => todos.filter((t) => !t.completed), [todos]);
  const done = useMemo(() => todos.filter((t) => t.completed), [todos]);
  const donePct = todos.length ? Math.round((done.length / todos.length) * 100) : 0;

  return (
    <section className="rail-panel">
      <header className="rail-panel-head">
        <div>
          <h4 className="rail-panel-title">📋 Today's Tasks</h4>
          <span className="rail-panel-sub">{open.length} open · {donePct}% done</span>
        </div>
      </header>

      <div className="rs-todo-progress">
        <div className="rs-todo-progress-bar" style={{ width: `${donePct}%` }} />
        <span className="rs-todo-progress-label">{donePct}%</span>
      </div>

      <div className="rs-todo-add">
        <input
          className="quick-input"
          placeholder="New task…"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void add()}
        />
        <button className="btn primary" disabled={busy} onClick={() => void add()}>+</button>
      </div>

      <ul className="habit-rail-list">
        {open.slice(0, 6).map((t) => (
          <li key={t.id}>
            <div className="rs-task-row">
              <button
                className="rs-task-check"
                onClick={() => void toggle(t.id)}
                aria-label="Mark done"
                style={{ borderColor: prioColor[t.priority] || prioColor.medium }}
              />
              <span className="rs-task-prio" style={{ background: prioColor[t.priority] || prioColor.medium }} />
              <span className="rs-task-title">{t.title}</span>
              <button className="icon-btn rs-task-del" onClick={() => void remove(t.id)} title="Delete">✕</button>
            </div>
          </li>
        ))}
        {open.length === 0 && (
          <li className="text-dim rail-panel-empty">All clear 🎉</li>
        )}
      </ul>
    </section>
  );
}

export function RightSidebar() {
  return (
    <aside className="right-rail" aria-label="Tasks, habits and productivity">
      <div className="right-rail-inner">
        <TasksPanel />
        <HabitPanel />
        <ChartsRailPanel />
      </div>
    </aside>
  );
}
