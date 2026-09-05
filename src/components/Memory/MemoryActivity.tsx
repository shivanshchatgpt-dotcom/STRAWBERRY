import { useEffect, useState } from "react";
import {
  api,
  type RuntimeSnapshot,
  type LedgerEntry,
  type GoalCandidate,
} from "../../lib/api";

/**
 * 🤖 Autonomous Activity — truthful observability for the autonomy
 * runtime. Every value comes from a real backend command.
 */
export function MemoryActivity() {
  const [snapshot, setSnapshot] = useState<RuntimeSnapshot | null>(null);
  const [ledger, setLedger] = useState<LedgerEntry[]>([]);
  const [goals, setGoals] = useState<GoalCandidate[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [snap, led, g] = await Promise.all([
        api.autonomyGetStats(),
        api.autonomyGetLedger(30),
        api.autonomyGetGoals(20),
      ]);
      setSnapshot(snap);
      setLedger(led);
      setGoals(g);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(id);
  }, []);

  async function handleStart() {
    try {
      // Use the Tauri runtime command to start autonomy.
      // (We re-use the existing autonomy_start command through the
      // legacy API surface.)
      // The legacy command path is window.__TAURI__.core.invoke.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const t = (window as any).__TAURI__;
      if (t?.core?.invoke) {
        await t.core.invoke("autonomy_start");
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }
  async function handlePause() {
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const t = (window as any).__TAURI__;
      if (t?.core?.invoke) {
        await t.core.invoke("autonomy_pause");
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="memory-activity">
      {error && <div className="memory-error-banner">{error}</div>}

      <h3 className="memory-section-title">Runtime</h3>
      {loading && !snapshot && <p>Loading…</p>}
      {snapshot && (
        <div className="memory-activity-grid">
          <div className="memory-stat-card">
            <div className="memory-stat-label">Mode</div>
            <div className="memory-stat-value">{snapshot.mode}</div>
          </div>
          <div className="memory-stat-card">
            <div className="memory-stat-label">Cycles total</div>
            <div className="memory-stat-value">{snapshot.stats.cyclesTotal}</div>
          </div>
          <div className="memory-stat-card">
            <div className="memory-stat-label">Events consumed</div>
            <div className="memory-stat-value">{snapshot.stats.eventsConsumedTotal}</div>
          </div>
          <div className="memory-stat-card">
            <div className="memory-stat-label">World state version</div>
            <div className="memory-stat-value">{snapshot.stats.worldStateVersion}</div>
          </div>
        </div>
      )}

      <div className="memory-activity-actions">
        <button className="btn" onClick={handleStart}>Start autonomy</button>
        <button className="btn btn-ghost" onClick={handlePause}>Pause autonomy</button>
      </div>

      {snapshot && (
        <section className="memory-section">
          <h3 className="memory-section-title">World state</h3>
          <dl className="memory-defs">
            <div className="memory-def-row">
              <dt>Active app</dt>
              <dd>{snapshot.worldState.activeApp ?? "—"}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Active project</dt>
              <dd>{snapshot.worldState.activeProject ?? "—"}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Active file</dt>
              <dd>{snapshot.worldState.activeFile?.path ?? "—"}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Workflow phase</dt>
              <dd>{snapshot.worldState.workflowPhase}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Build state</dt>
              <dd>{snapshot.worldState.buildState}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Test state</dt>
              <dd>{snapshot.worldState.testState}</dd>
            </div>
          </dl>
        </section>
      )}

      <section className="memory-section">
        <h3 className="memory-section-title">Recent ledger ({ledger.length})</h3>
        {ledger.length === 0 ? (
          <p className="memory-muted">No ledger entries yet.</p>
        ) : (
          <ul className="memory-ledger-list">
            {ledger.map((e) => (
              <li key={e.id} className="memory-ledger-item">
                <span className={`memory-ledger-decision memory-ledger-${e.decision}`}>
                  {e.decision}
                </span>
                <span className="memory-ledger-cap">{e.capabilityId}</span>
                <span className="memory-ledger-reason">{e.reason}</span>
                {e.score != null && (
                  <span className="memory-ledger-score">{e.score.toFixed(2)}</span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="memory-section">
        <h3 className="memory-section-title">Goal candidates ({goals.length})</h3>
        {goals.length === 0 ? (
          <p className="memory-muted">
            No goal candidates detected yet. The autonomy engine scans
            todos, errors, resume points, and project context to produce
            candidates.
          </p>
        ) : (
          <ul className="memory-goal-list">
            {goals.map((g) => (
              <li key={g.goalId} className="memory-goal-item">
                <div className="memory-goal-row">
                  <span className="memory-goal-priority">{g.priority}</span>
                  <span className="memory-goal-title">{g.title}</span>
                  <span className="memory-goal-confidence">
                    conf {g.confidence.toFixed(2)}
                  </span>
                </div>
                {g.project && (
                  <div className="memory-goal-project">📁 {g.project}</div>
                )}
                {g.evidence.length > 0 && (
                  <ul className="memory-goal-evidence">
                    {g.evidence.map((ev, i) => (
                      <li key={i}>
                        <span className="memory-goal-ev-kind">{ev.kind}</span>{" "}
                        {ev.summary}
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
