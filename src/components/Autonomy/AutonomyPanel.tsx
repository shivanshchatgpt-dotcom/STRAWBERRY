import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { autonomy } from "../../lib/types";

const PHASE_LABELS: Record<string, string> = {
  idle: "💤 Idle",
  coding: "💻 Coding",
  reading: "📖 Reading",
  searching: "🔍 Searching",
  debugging: "🐛 Debugging",
  planning: "📋 Planning",
  reviewing: "👀 Reviewing",
  writing: "✍️ Writing",
  unknown: "❓ Unknown",
};

const MODE_LABELS: Record<string, string> = {
  stopped: "⏹ Stopped",
  running: "▶ Running",
  paused: "⏸ Paused",
};

const STATE_LABELS: Record<string, string> = {
  unknown: "—",
  idle: "Idle",
  running: "Running",
  succeeded: "✅ Succeeded",
  failed: "❌ Failed",
};

function fmtBuild(s: autonomy.BuildState): string {
  if (typeof s === "string") return STATE_LABELS[s] ?? s;
  if ("failed" in s) return `❌ ${s.failed.message}`;
  return JSON.stringify(s);
}

function fmtTest(s: autonomy.TestState): string {
  if (typeof s === "string") return STATE_LABELS[s] ?? s;
  if ("passed" in s) return `✅ ${s.passed.count} passed`;
  if ("failed" in s) return `❌ ${s.failed.passed} passed, ${s.failed.failed} failed`;
  return JSON.stringify(s);
}

function fmtTs(ms: number): string {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}

export function AutonomyPanel() {
  const [snap, setSnap] = useState<autonomy.RuntimeSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [caps, setCaps] = useState<autonomy.CapabilityState[]>([]);
  const [ledger, setLedger] = useState<autonomy.LedgerEntry[]>([]);
  const [goals, setGoals] = useState<autonomy.GoalCandidate[]>([]);
  const [plans, setPlans] = useState<autonomy.PlannedResult[]>([]);
  const [showLedger, setShowLedger] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const s = await api.autonomyGetState();
      setSnap(s);
      setError(null);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  }, []);

  const refreshCaps = useCallback(async () => {
    try {
      const [c, l, g, pl] = await Promise.all([
        api.listCapabilities(),
        api.getCapabilityLedger(30),
        api.getGoalCandidates(),
        api.getPlans(),
      ]);
      setCaps(c);
      setLedger(l);
      setGoals(g);
      setPlans(pl);
    } catch {
      /* silent */
    }
  }, []);

  useEffect(() => {
    void refresh();
    void refreshCaps();
    const t = setInterval(() => void refresh(), 2000);
    return () => clearInterval(t);
  }, [refresh, refreshCaps]);

  const toggleCap = async (id: string, enabled: boolean) => {
    setBusy(true);
    try {
      await api.setCapabilityEnabled(id, enabled);
      await refreshCaps();
    } finally {
      setBusy(false);
    }
  };

  const setCapInterval = async (id: string, secs: number) => {
    setBusy(true);
    try {
      await api.setCapabilityInterval(id, secs);
      await refreshCaps();
    } finally {
      setBusy(false);
    }
  };

  const start = async () => { setBusy(true); try { await api.autonomyStart(); await refresh(); } finally { setBusy(false); } };
  const pause = async () => { setBusy(true); try { await api.autonomyPause(); await refresh(); } finally { setBusy(false); } };
  const resume = async () => { setBusy(true); try { await api.autonomyResume(); await refresh(); } finally { setBusy(false); } };
  const shutdown = async () => { setBusy(true); try { await api.autonomyShutdown(); await refresh(); } finally { setBusy(false); } };
  const tick = async () => { setBusy(true); try { await api.autonomyRunCycle(32); await refresh(); } finally { setBusy(false); } };

  // Demo publishers to exercise the world-state pipeline
  const demoHeartbeat = async () => { await api.autonomyPublish("heartbeat", { source: "panel" }); };
  const demoFile = async () => {
    await api.autonomyPublish("file_opened", { path: "src/main.rs", project: "strawberry" });
  };
  const demoApp = async () => {
    await api.autonomyPublish("active_app_changed", { from: null, to: "vscode" });
  };
  const demoError = async () => {
    await api.autonomyPublish("error_observed", { message: "E0308 mismatched types", source: "build" });
  };

  if (error) {
    return (
      <div className="panel" style={{ borderColor: "#ef444440" }}>
        <h3 className="panel-title">🤖 Autonomy</h3>
        <p style={{ color: "#ef4444", fontSize: 13 }}>{error}</p>
        <button className="btn primary" onClick={() => void refresh()} disabled={busy}>Retry</button>
      </div>
    );
  }

  if (!snap) {
    return (
      <div className="panel">
        <h3 className="panel-title">🤖 Autonomy</h3>
        <div className="loading-block"><span className="spinner" /> Booting autonomy…</div>
      </div>
    );
  }

  const { mode, stats, worldState: ws } = snap;

  return (
    <section className="panel autonomy-panel" aria-label="Autonomous Runtime">
      <header className="autonomy-header">
        <h3 className="panel-title">🤖 Autonomy</h3>
        <div className="autonomy-mode">
          <span className={`autonomy-dot mode-${mode}`} />
          <span className="text-dim" style={{ fontSize: 12 }}>{MODE_LABELS[mode] ?? mode}</span>
        </div>
      </header>

      <p className="text-dim" style={{ fontSize: 12, margin: "4px 0 10px" }}>
        Rust-native, no-LLM, event-driven worker. Phase 1: observe + world-state. Phases 2–18 will plug goal engine, planner, safety, executor, verifier, learning behind the same runtime.
      </p>

      <div className="autonomy-controls">
        {mode !== "running" && (
          <button className="btn primary" onClick={() => void (mode === "paused" ? resume() : start())} disabled={busy}>
            {mode === "paused" ? "▶ Resume" : "▶ Start"}
          </button>
        )}
        {mode === "running" && (
          <button className="btn" onClick={() => void pause()} disabled={busy}>⏸ Pause</button>
        )}
        <button className="btn" onClick={() => void tick()} disabled={busy}>⏭ Run 1 Cycle</button>
        <button className="btn" onClick={() => void shutdown()} disabled={busy}>⏹ Shutdown</button>
      </div>

      <div className="autonomy-stats">
        <div className="stat-card">
          <div className="stat-value">{stats.cyclesTotal}</div>
          <div className="stat-label">Cycles</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.cyclesWithAction}</div>
          <div className="stat-label">With Action</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.eventsConsumedTotal}</div>
          <div className="stat-label">Events</div>
        </div>
        <div className="stat-card">
          <div className="stat-value">{stats.uptimeSecs}s</div>
          <div className="stat-label">Uptime</div>
        </div>
      </div>

      <div className="autonomy-world">
        <h4 className="panel-title" style={{ fontSize: 12.5, margin: "10px 0 6px" }}>🌍 World State <span className="text-dim">v{ws.version}</span></h4>
        <div className="ws-grid">
          <div className="ws-row"><span className="ws-key">App</span><span className="ws-val">{ws.activeApp ?? "—"}</span></div>
          <div className="ws-row"><span className="ws-key">Window</span><span className="ws-val">{ws.activeWindowTitle ?? "—"}</span></div>
          <div className="ws-row"><span className="ws-key">Project</span><span className="ws-val">{ws.activeProject ?? "—"}</span></div>
          <div className="ws-row"><span className="ws-key">File</span><span className="ws-val">{ws.activeFile?.path ?? "—"}</span></div>
          <div className="ws-row"><span className="ws-key">Phase</span><span className="ws-val">{PHASE_LABELS[ws.workflowPhase] ?? ws.workflowPhase}</span></div>
          <div className="ws-row"><span className="ws-key">Build</span><span className="ws-val">{fmtBuild(ws.buildState)}</span></div>
          <div className="ws-row"><span className="ws-key">Test</span><span className="ws-val">{fmtTest(ws.testState)}</span></div>
          <div className="ws-row"><span className="ws-key">Errors</span><span className="ws-val">{ws.recentErrors.length}</span></div>
        </div>

        {ws.recentErrors.length > 0 && (
          <div className="ws-errors">
            <h5 className="panel-title" style={{ fontSize: 11.5, margin: "8px 0 4px", textTransform: "uppercase", letterSpacing: 0.5 }}>Recent errors</h5>
            {ws.recentErrors.slice(0, 5).map((e: autonomy.ErrorState, i: number) => (
              <div key={i} className="ws-error-item">
                <span className="error-source">{e.source}</span>
                <span className="error-msg">{e.message}</span>
                <span className="error-time">{fmtTs(e.atMs)}</span>
              </div>
            ))}
          </div>
        )}

        {ws.recentAppSwitches.length > 0 && (
          <div className="ws-stream">
            <h5 className="panel-title" style={{ fontSize: 11.5, margin: "8px 0 4px", textTransform: "uppercase", letterSpacing: 0.5 }}>App switches</h5>
            <div className="ws-stream-list">
              {ws.recentAppSwitches.slice(0, 8).map((a: string, i: number) => (
                <span key={i} className="ws-chip">{a}</span>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="autonomy-demo">
        <h4 className="panel-title" style={{ fontSize: 12, margin: "10px 0 6px", textTransform: "uppercase", letterSpacing: 0.5 }}>Inject demo events</h4>
        <div className="autonomy-demo-row">
          <button className="btn" onClick={() => void demoHeartbeat()}>💓 Heartbeat</button>
          <button className="btn" onClick={() => void demoApp()}>🪟 App switch</button>
          <button className="btn" onClick={() => void demoFile()}>📄 Open file</button>
          <button className="btn" onClick={() => void demoError()}>❌ Error</button>
        </div>
      </div>

      {/* ── Goal Engine — deterministic candidates from real evidence ── */}
      <div className="goal-engine">
        <h4 className="panel-title" style={{ fontSize: 12, margin: "12px 0 6px", textTransform: "uppercase", letterSpacing: 0.5 }}>
          🎯 Goal candidates ({goals.length})
        </h4>
        {goals.length === 0 && (
          <p className="text-dim" style={{ fontSize: 11.5 }}>
            Koi evidence-backed goal nahi — task/error/resume se derive hote hain.
          </p>
        )}
        <ul className="goal-list">
          {goals.slice(0, 6).map((g) => (
            <li key={g.goalId} className={`goal-row prio-${g.priority}${g.status !== "candidate" ? " inactive" : ""}`}>
              <span className="goal-title" title={g.description}>{g.title}</span>
              <span className="goal-project">{g.project ?? "—"}</span>
              <span className="goal-conf" title="confidence">{(g.confidence * 100).toFixed(0)}%</span>
              <span className={`goal-status s-${g.status}`}>{g.status}</span>
            </li>
          ))}
        </ul>
      </div>

      {/* ── Planner — deterministic non-executing plans (Phase 8) ── */}
      {plans.length > 0 && (
        <div className="plan-engine">
          <h4 className="panel-title" style={{ fontSize: 12, margin: "12px 0 6px", textTransform: "uppercase", letterSpacing: 0.5 }}>
            🗺️ Plans ({plans.filter((p) => p.kind === "plan").length} · {plans.filter((p) => p.kind === "rejected").length} rejected)
          </h4>
          <ul className="plan-list">
            {plans.slice(0, 4).map((r, i) => (
              <li key={i} className="plan-row">
                {r.kind === "plan" ? (
                  <>
                    <span className="plan-title">{r.value.title}</span>
                    <span className="plan-meta">
                      {r.value.steps.length} steps · cost {r.value.estimatedCost} · conf {(r.value.confidence * 100).toFixed(0)}%
                    </span>
                    <ol className="plan-steps">
                      {r.value.steps.slice(0, 4).map((s) => (
                        <li key={s.stepId} title={s.purpose}>
                          {s.action === "requires_approval" ? "🔒" : s.action === "prepare" ? "🧰" : "🔍"}{" "}
                          {s.capability} — {s.expectedResult}
                        </li>
                      ))}
                    </ol>
                  </>
                ) : (
                  <>
                    <span className="plan-title dim">✖ {r.value.reason}</span>
                  </>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* ── Capability Registry — the ONE catalog the scheduler serves ── */}
      <div className="cap-registry">
        <h4 className="panel-title" style={{ fontSize: 12, margin: "12px 0 6px", textTransform: "uppercase", letterSpacing: 0.5 }}>
          🧩 Capabilities ({caps.filter((c) => c.enabled).length}/{caps.length} on)
        </h4>
        <ul className="cap-list">
          {caps.map((c) => (
            <li key={c.id} className={`cap-row${c.enabled ? "" : " off"}`}>
              <label className="cap-toggle">
                <input
                  type="checkbox"
                  checked={c.enabled}
                  disabled={busy}
                  onChange={(e) => void toggleCap(c.id, e.target.checked)}
                />
              </label>
              <div className="cap-meta">
                <span className="cap-name">{c.name}</span>
                <span className="cap-goal">{c.goal}</span>
              </div>
              <span className={`cap-risk risk-${c.risk}`} title={`risk: ${c.risk}`}>
                {c.risk === "low" ? "🟢" : c.risk === "medium" ? "🟡" : c.risk === "high" ? "🔴" : "⛔"}
              </span>
              <span className="cap-layer">{c.layer}</span>
              {c.defaultIntervalSecs > 0 && (
                <select
                  className="cap-interval"
                  value={c.intervalSecs}
                  disabled={busy || !c.enabled}
                  onChange={(e) => void setCapInterval(c.id, Number(e.target.value))}
                  title="cadence override"
                >
                  {[15, 30, 60, 120, 300, 600, 900, 1800, 3600]
                    .filter((s) => s >= (c.defaultIntervalSecs > 300 ? 300 : 15))
                    .map((s) => (
                      <option key={s} value={s}>
                        {s < 60 ? `${s}s` : `${s / 60}m`}
                      </option>
                    ))}
                </select>
              )}
            </li>
          ))}
        </ul>

        <button className="btn" style={{ marginTop: 8 }} onClick={() => setShowLedger((v) => !v)}>
          {showLedger ? "Hide" : "📜"} Decision ledger
        </button>
        {showLedger && (
          <ul className="ledger-list">
            {ledger.length === 0 && (
              <li className="text-dim" style={{ fontSize: 11.5 }}>No decisions logged yet.</li>
            )}
            {ledger.map((l) => (
              <li key={l.id} className="ledger-row">
                <span className={`ledger-decision d-${l.decision}`}>{l.decision}</span>
                <span className="ledger-cap">{l.capabilityId}</span>
                <span className="ledger-reason">{l.reason}</span>
                {l.score != null && (
                  <span className="ledger-score">{l.score.toFixed(2)}</span>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
