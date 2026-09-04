import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import type { planner, ghost, alpha } from "../../lib/api";

/**
 * 🍓 Productivity charts rail — real data, click-to-expand.
 *
 * Small charts live in the right rail; clicking one expands it in place
 * with a description of what it shows and the underlying numbers.
 *
 * Data sources (all offline, no LLM):
 *   • Focus stats   — planner focus sessions (getFocusStats)
 *   • Habits        — completion per habit (getHabits)
 *   • Tasks         — todos by priority (getTodos)
 *   • Alpha         — hunter candidates by source (listAlphaCandidates)
 *   • Ghost         — attention heatmap (ghostGetSnapshot)
 */

// ─────────────────────────── data hooks ───────────────────────────

interface RailData {
  todos: planner.Todo[];
  habits: planner.Habit[];
  events: planner.ScheduleEvent[];
  focus: planner.FocusStats | null;
  alphas: alpha.AlphaCandidate[];
  ghostSnap: ghost.GhostSnapshot | null;
}

function useRailData(): RailData {
  const [d, setD] = useState<RailData>({
    todos: [], habits: [], events: [], focus: null, alphas: [], ghostSnap: null,
  });

  const refresh = useCallback(async () => {
    try {
      const [todos, habits, events, focus, alphas, ghostSnap] = await Promise.all([
        api.getTodos().catch(() => [] as planner.Todo[]),
        api.getHabits().catch(() => [] as planner.Habit[]),
        api.getSchedule().catch(() => [] as planner.ScheduleEvent[]),
        api.getFocusStats().catch(() => null),
        api.listAlphaCandidates().catch(() => [] as alpha.AlphaCandidate[]),
        api.ghostGetSnapshot().catch(() => null),
      ]);
      setD({ todos, habits, events, focus, alphas, ghostSnap });
    } catch {
      /* silent */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 60_000);
    return () => clearInterval(t);
  }, [refresh]);

  return d;
}

// ─────────────────────────── chart models ───────────────────────────

interface ChartDef {
  key: string;
  emoji: string;
  title: string;
  subtitle: string;
  desc: string;
  bars: Array<{ label: string; value: number; hint?: string }>;
}

function buildCharts(d: RailData): ChartDef[] {
  const charts: ChartDef[] = [];

  // 1. Tasks by status/priority
  const open = d.todos.filter((t) => !t.completed);
  const done = d.todos.filter((t) => t.completed);
  const hi = open.filter((t) => t.priority === "high").length;
  const med = open.filter((t) => t.priority === "medium").length;
  const lo = open.filter((t) => t.priority === "low").length;
  charts.push({
    key: "tasks",
    emoji: "📋",
    title: "Tasks",
    subtitle: `${open.length} open · ${done.length} done`,
    desc: "Open tasks split by priority. High-priority first keeps the day focused; the done count is your completion momentum.",
    bars: [
      { label: "High", value: hi, hint: "urgent / important" },
      { label: "Medium", value: med, hint: "normal pace" },
      { label: "Low", value: lo, hint: "whenever" },
      { label: "Done", value: done.length, hint: "completed" },
    ],
  });

  // 2. Habit streaks
  const streakOf = (dates: string[]) => {
    if (!dates.length) return 0;
    const set = new Set(dates);
    const day = new Date();
    const key = (x: Date) => x.toISOString().slice(0, 10);
    if (!set.has(key(day))) day.setDate(day.getDate() - 1);
    let n = 0;
    while (set.has(key(day))) { n++; day.setDate(day.getDate() - 1); }
    return n;
  };
  charts.push({
    key: "habits",
    emoji: "🔥",
    title: "Habit streaks",
    subtitle: `${d.habits.length} habits tracked`,
    desc: "Consecutive days each habit has been completed. Long bars are compounding wins — protect them first on busy days.",
    bars: d.habits.slice(0, 8).map((h) => ({
      label: h.name,
      value: streakOf(h.completedDates),
      hint: `${h.completedDates.length} total days`,
    })),
  });

  // 3. Focus sessions
  if (d.focus) {
    charts.push({
      key: "focus",
      emoji: "⏱",
      title: "Focus",
      subtitle: `${d.focus.todaySessions} today · ${d.focus.todayMinutes} min`,
      desc: "Deep-work sessions from the Planner focus timer. Today vs lifetime totals — consistent daily blocks beat rare marathons.",
      bars: [
        { label: "Today", value: d.focus.todayMinutes },
        { label: "Sessions", value: d.focus.todaySessions },
        { label: "Total min", value: d.focus.totalMinutes },
        { label: "All sess.", value: d.focus.sessions },
      ],
    });
  }

  // 4. Alpha candidates by source
  if (d.alphas.length > 0) {
    const bySrc = new Map<string, number>();
    for (const a of d.alphas) bySrc.set(a.source, (bySrc.get(a.source) ?? 0) + 1);
    charts.push({
      key: "alpha",
      emoji: "🎯",
      title: "Alpha sources",
      subtitle: `${d.alphas.length} candidates`,
      desc: "Where your tracked free-model candidates came from — HackerNews, Reddit, GitHub, HuggingFace, OpenRouter, Product Hunt.",
      bars: [...bySrc.entries()].map(([label, value]) => ({ label, value })),
    });
  }

  // 5. Ghost attention peak hours
  if (d.ghostSnap && d.ghostSnap.heatmap.length > 0) {
    const byHour = new Map<number, number>();
    for (const c of d.ghostSnap.heatmap) {
      byHour.set(c.hour, (byHour.get(c.hour) ?? 0) + c.count);
    }
    const top = [...byHour.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6)
      .map(([h, n]) => ({ label: `${String(h).padStart(2, "0")}:00`, value: n, hint: "events" }));
    charts.push({
      key: "ghost",
      emoji: "👻",
      title: "Peak hours",
      subtitle: `${d.ghostSnap.stats.totalEvents} events logged`,
      desc: "When you actually open, search and capture knowledge — your real productivity rhythm from the Ghost attention heatmap.",
      bars: top,
    });
  }

  // 6. Upcoming events (next 7 days)
  const weekAhead = d.events.filter((e) => {
    const start = new Date(e.startTime).getTime();
    const now = Date.now();
    return start >= now && start <= now + 7 * 86_400_000;
  });
  if (weekAhead.length > 0) {
    charts.push({
      key: "week",
      emoji: "📅",
      title: "This week",
      subtitle: `${weekAhead.length} events ahead`,
      desc: "Calendar events in the next 7 days. Each bar is one day's load — spot collision days before they happen.",
      bars: buildWeekBars(weekAhead),
    });
  }

  return charts;
}

function buildWeekBars(events: planner.ScheduleEvent[]) {
  const days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const counts = new Array(7).fill(0) as number[];
  const now = Date.now();
  for (const e of events) {
    const t = new Date(e.startTime).getTime();
    if (t >= now) {
      const d = new Date(t);
      counts[d.getDay()]++;
    }
  }
  return days.map((label, i) => ({ label, value: counts[i], hint: "events" }));
}

// ─────────────────────────── component ───────────────────────────

export function ChartsRailPanel() {
  const data = useRailData();
  const charts = useMemo(() => buildCharts(data), [data]);
  const [expanded, setExpanded] = useState<string | null>(null);

  return (
    <section className="rail-panel rail-charts">
      <header className="rail-panel-head">
        <div>
          <h4 className="rail-panel-title">📊 Productivity</h4>
          <span className="rail-panel-sub">Click a chart to expand</span>
        </div>
      </header>

      {charts.length === 0 && (
        <p className="text-dim rail-panel-empty">
          No data yet — add tasks, habits or run a focus session.
        </p>
      )}

      <div className="rail-charts-list">
        {charts.map((c) => {
          const open = expanded === c.key;
          return (
            <article
              key={c.key}
              className={`rail-chart${open ? " open" : ""}`}
              onClick={() => setExpanded(open ? null : c.key)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === "Enter" && setExpanded(open ? null : c.key)}
              aria-expanded={open}
            >
              <header className="rail-chart-head">
                <span className="rail-chart-emoji" aria-hidden>{c.emoji}</span>
                <div className="rail-chart-meta">
                  <span className="rail-chart-title">{c.title}</span>
                  <span className="rail-chart-sub">{c.subtitle}</span>
                </div>
                <span className="rail-chart-arrow" aria-hidden>{open ? "−" : "+"}</span>
              </header>

              <div className="rail-chart-bars">
                {c.bars.map((b, i) => {
                  const max = Math.max(...c.bars.map((x) => x.value), 1);
                  const pct = Math.round((b.value / max) * 100);
                  return (
                    <div
                      key={`${b.label}-${i}`}
                      className="rail-chart-bar-row"
                      title={b.hint ? `${b.label} — ${b.hint}` : b.label}
                    >
                      <span className="rail-chart-bar-label">{b.label}</span>
                      <div className="rail-chart-bar-track">
                        <span
                          className="rail-chart-bar-fill"
                          style={{ width: `${Math.max(4, pct)}%` }}
                          data-pct={pct}
                        />
                      </div>
                      <span className="rail-chart-bar-value">{b.value}</span>
                    </div>
                  );
                })}
              </div>

              {open && (
                <p className="rail-chart-desc">{c.desc}</p>
              )}
            </article>
          );
        })}
      </div>
    </section>
  );
}
