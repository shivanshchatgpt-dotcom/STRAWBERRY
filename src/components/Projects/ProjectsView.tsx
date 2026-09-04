import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type {
  IntelligentResumeData,
  ProjectBrainData,
  WhatChangedData,
} from "../../lib/api";

/**
 * 🌳 Projects — Project Brain / What Changed / Intelligent Resume.
 * Phase C + D + E of the platform roadmap, one view.
 *
 * All data is deterministic aggregation over existing storage — no LLM.
 */

const fmtSeen = (secs: number) => {
  if (!secs) return "—";
  try {
    return new Date(secs * 1000).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return String(secs);
  }
};

const fmtSince = (iso: string) => {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
};

export function ProjectsView() {
  const [brain, setBrain] = useState<ProjectBrainData | null>(null);
  const [resume, setResume] = useState<IntelligentResumeData | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const [b, r] = await Promise.all([
        api.getProjectBrain(),
        api.getIntelligentResume(),
      ]);
      setBrain(b);
      setResume(r);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="content dashboard">
      <header className="dash-head">
        <div>
          <h1 className="dash-title">🌳 Projects</h1>
          <div className="meta-line">
            Project Brain · What Changed · Intelligent Resume — all deterministic
          </div>
        </div>
        <button className="btn" onClick={() => void refresh()} disabled={busy}>
          {busy ? <span className="spinner" /> : null} ↻ Refresh
        </button>
      </header>

      {error && <div className="dash-error">⚠️ {error}</div>}

      {/* ------------------ Intelligent Resume ------------------ */}
      {resume && (
        <section className="panel" aria-label="Intelligent resume">
          <h3 className="panel-title">⏮️ Intelligent Resume</h3>
          <p className="resume-headline">{resume.narrative.headline}</p>

          <div className="resume-changed">
            <strong>Since {fmtSince(resume.changes.since)}:</strong>{" "}
            {resume.changes.summary}
            <div className="text-dim" style={{ fontSize: 11, marginTop: 2 }}>
              baseline: {resume.changes.baselineNote}
            </div>
          </div>

          <ol className="resume-plan">
            {resume.narrative.plan.map((p, i) => (
              <li key={i}>{p}</li>
            ))}
          </ol>
        </section>
      )}

      {/* ------------------ What Changed detail ------------------ */}
      {resume && <ChangedDetail changes={resume.changes} />}

      {/* ------------------ Project cards ------------------ */}
      {brain && brain.projects.length === 0 && (
        <section className="panel">
          <p className="text-dim" style={{ fontSize: 13 }}>
            Koi project discover nahi hua. Freeze Now (workspace snapshot) chala
            lo — VS Code folders aur terminal cwds se projects auto-detect
            honge.
          </p>
        </section>
      )}

      <div className="brief-grid">
        {brain?.projects.map((p) => {
          const open = expanded === p.path;
          return (
            <article
              key={p.path}
              className={`card card-project${open ? " open" : ""}`}
              onClick={() => setExpanded(open ? null : p.path)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === "Enter" && setExpanded(open ? null : p.path)}
            >
              <h3 className="card-title">
                {p.origin === "both" ? "🖥️" : p.origin === "vscode" ? "🧩" : "⌨️"} {p.name}
              </h3>
              <div className="card-lines">
                <li>📍 {p.path}</li>
                <li>🕒 last seen {fmtSeen(p.lastSeenAt)}</li>
                <li>🎯 {p.nextLikelyAction}</li>
                {p.openTasks.length > 0 && (
                  <li>📋 {p.openTasks.length} open task(s)</li>
                )}
                {p.recentErrors.length > 0 && (
                  <li>❌ {p.recentErrors.length} captured error(s)</li>
                )}
                <li>✅ {p.tasksDone} tasks done all-time</li>
              </div>

              {open && (
                <div className="project-detail">
                  {p.openTasks.length > 0 && (
                    <>
                      <h5>Open tasks</h5>
                      <ul>
                        {p.openTasks.map((t, i) => <li key={i}>{t}</li>)}
                      </ul>
                    </>
                  )}
                  {p.recentErrors.length > 0 && (
                    <>
                      <h5>Recent errors</h5>
                      <ul>
                        {p.recentErrors.map((e, i) => <li key={i}>{e}</li>)}
                      </ul>
                    </>
                  )}
                  {p.decisions.length > 0 && (
                    <>
                      <h5>Decisions / intents</h5>
                      <ul>
                        {p.decisions.map((d, i) => <li key={i}>{d}</li>)}
                      </ul>
                    </>
                  )}
                  {p.activity.length > 0 && (
                    <>
                      <h5>Activity</h5>
                      <ul>
                        {p.activity.map(([k, n], i) => (
                          <li key={i}>{k} × {n}</li>
                        ))}
                      </ul>
                    </>
                  )}
                </div>
              )}
            </article>
          );
        })}
      </div>
    </div>
  );
}

function ChangedDetail({ changes }: { changes: WhatChangedData }) {
  const sections: Array<{ emoji: string; title: string; items: string[] }> = [
    { emoji: "✅", title: "Tasks completed", items: changes.tasksCompleted },
    { emoji: "🆕", title: "Tasks added", items: changes.tasksAdded },
    { emoji: "📥", title: "New captures", items: changes.newCaptures },
    { emoji: "💬", title: "Chats touched", items: changes.newChats },
    { emoji: "🔥", title: "Habits done", items: changes.habitsDone },
    { emoji: "📅", title: "Events", items: changes.newEvents },
  ].filter((s) => s.items.length > 0);

  if (sections.length === 0) return null;

  return (
    <section className="panel" aria-label="What changed">
      <h3 className="panel-title">🔄 What Changed</h3>
      <div className="changed-grid">
        {sections.map((s) => (
          <div key={s.title} className="changed-col">
            <h5>{s.emoji} {s.title}</h5>
            <ul>
              {s.items.map((it, i) => <li key={i}>{it}</li>)}
            </ul>
          </div>
        ))}
      </div>
    </section>
  );
}
