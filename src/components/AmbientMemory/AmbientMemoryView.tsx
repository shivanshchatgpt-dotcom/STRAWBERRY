import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/appStore";
import type {
  AmbientEvent,
  AmbientStats,
  DeterministicReport,
  SymbolicAnalysis,
} from "../../lib/types";

const SAMPLE_TS = `import { useState } from "react";
import { api } from "./api";

export interface UserState {
  id: string;
  role: string;
}

export function handleUserAuth(token: string): UserState {
  if (!token) throw new Error("Invalid session token");
  return { id: "usr_101", role: "admin" };
}

export const renderHeader = (title: string) => title.toUpperCase();`;

export function AmbientMemoryView() {
  const showToast = useAppStore((s) => s.showToast);

  const [stats, setStats] = useState<AmbientStats | null>(null);
  const [events, setEvents] = useState<AmbientEvent[]>([]);
  const [report, setReport] = useState<DeterministicReport | null>(null);
  const [loading, setLoading] = useState(true);

  // AST analysis form
  const [lang, setLang] = useState("typescript");
  const [source, setSource] = useState(SAMPLE_TS);
  const [astResult, setAstResult] = useState<SymbolicAnalysis | null>(null);
  const [analyzing, setAnalyzing] = useState(false);

  const refreshData = async () => {
    setLoading(true);
    try {
      const [evs, st, rep] = await Promise.all([
        api.getAmbientEvents(50).catch(() => []),
        api.getAmbientStats().catch(() => null),
        api.generateDeterministicReport().catch(() => null),
      ]);
      setEvents(evs);
      setStats(st);
      setReport(rep);
    } catch {
      showToast("error", "Failed to load ambient memory data");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refreshData();
  }, []);

  const handleRunAst = async () => {
    if (!source.trim()) return;
    setAnalyzing(true);
    try {
      const result = await api.analyzeCodeAst(lang, source);
      setAstResult(result);
      showToast("success", `Extracted ${result.functions.length} functions & ${result.typesOrClasses.length} types`);

      // Save AST event into ambient memory
      await api.recordAmbientEvent(
        "symbolic_ast",
        `AST Extraction (${result.language})`,
        `Extracted ${result.functions.length} fn(s), ${result.typesOrClasses.length} type(s), ${result.imports.length} import(s)`,
        "Strawberry AST Engine",
        JSON.stringify(result),
      );
      await refreshData();
    } catch {
      showToast("error", "Failed to run AST analysis");
    } finally {
      setAnalyzing(false);
    }
  };

  return (
    <div className="dashboard-view" style={{ padding: "1.5rem", overflowY: "auto", height: "100%" }}>
      <header style={{ marginBottom: "1.5rem", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div>
          <h1 style={{ margin: 0, fontSize: "1.5rem", fontWeight: 700, display: "flex", alignItems: "center", gap: "0.5rem" }}>
            🧠 Ambient Memory & Symbolic Graph
          </h1>
          <p style={{ margin: "0.25rem 0 0", color: "var(--text-muted)", fontSize: "0.9rem" }}>
            Continuous local-first knowledge fabric with zero-latency deterministic AST parsing (OS Independent)
          </p>
        </div>
        <button className="btn" onClick={() => void refreshData()} disabled={loading}>
          🔄 Refresh Fabric
        </button>
      </header>

      {/* Stats row */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))", gap: "1rem", marginBottom: "1.5rem" }}>
        <div className="card" style={{ padding: "1rem" }}>
          <div style={{ color: "var(--text-muted)", fontSize: "0.8rem", textTransform: "uppercase", fontWeight: 600 }}>
            Total Ambient Events
          </div>
          <div style={{ fontSize: "1.75rem", fontWeight: 700, marginTop: "0.25rem" }}>
            {stats ? stats.totalEvents : events.length}
          </div>
        </div>
        <div className="card" style={{ padding: "1rem" }}>
          <div style={{ color: "var(--text-muted)", fontSize: "0.8rem", textTransform: "uppercase", fontWeight: 600 }}>
            AST Symbolic Scans
          </div>
          <div style={{ fontSize: "1.75rem", fontWeight: 700, marginTop: "0.25rem", color: "#ec4899" }}>
            {stats ? stats.astEvents : 0}
          </div>
        </div>
        <div className="card" style={{ padding: "1rem" }}>
          <div style={{ color: "var(--text-muted)", fontSize: "0.8rem", textTransform: "uppercase", fontWeight: 600 }}>
            OS Platform Node
          </div>
          <div style={{ fontSize: "1.25rem", fontWeight: 700, marginTop: "0.5rem", textTransform: "capitalize" }}>
            🖥️ {stats?.platform || "Local Node"}
          </div>
        </div>
        <div className="card" style={{ padding: "1rem" }}>
          <div style={{ color: "var(--text-muted)", fontSize: "0.8rem", textTransform: "uppercase", fontWeight: 600 }}>
            Privacy Guarantee
          </div>
          <div style={{ fontSize: "1.1rem", fontWeight: 600, marginTop: "0.5rem", color: "#10b981" }}>
            🔒 100% Offline / Local
          </div>
        </div>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1.5rem" }}>
        {/* AST Inspector panel */}
        <div className="card" style={{ padding: "1.25rem" }}>
          <h2 style={{ fontSize: "1.1rem", margin: "0 0 1rem", fontWeight: 600, display: "flex", alignItems: "center", gap: "0.5rem" }}>
            ⚡ Zero-Latency Multi-Language AST Engine
          </h2>
          <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.75rem" }}>
            <select
              value={lang}
              onChange={(e) => setLang(e.target.value)}
              style={{
                padding: "0.4rem 0.75rem",
                borderRadius: "6px",
                border: "1px solid var(--border)",
                background: "var(--bg-input)",
                color: "var(--text)",
              }}
            >
              <option value="typescript">TypeScript / JS</option>
              <option value="python">Python</option>
              <option value="rust">Rust</option>
              <option value="go">Go</option>
            </select>
            <button className="btn primary" onClick={() => void handleRunAst()} disabled={analyzing}>
              {analyzing ? "Analyzing..." : "Parse Structural AST"}
            </button>
          </div>

          <textarea
            value={source}
            onChange={(e) => setSource(e.target.value)}
            rows={8}
            placeholder="Paste source code here to extract AST functions, imports, types & errors..."
            style={{
              width: "100%",
              fontFamily: "monospace",
              fontSize: "0.85rem",
              padding: "0.75rem",
              borderRadius: "6px",
              border: "1px solid var(--border)",
              background: "var(--bg-input)",
              color: "var(--text)",
              resize: "vertical",
            }}
          />

          {astResult && (
            <div style={{ marginTop: "1rem", background: "var(--bg-subtle)", padding: "0.75rem", borderRadius: "6px", fontSize: "0.85rem" }}>
              <div style={{ fontWeight: 600, marginBottom: "0.5rem" }}>
                📊 Extracted Structural Symbols ({astResult.language}) — {astResult.totalLines} lines
              </div>

              {astResult.imports.length > 0 && (
                <div style={{ marginBottom: "0.5rem" }}>
                  <strong>Imports:</strong> {astResult.imports.join(", ")}
                </div>
              )}

              {astResult.functions.length > 0 && (
                <div style={{ marginBottom: "0.5rem" }}>
                  <strong>Functions ({astResult.functions.length}):</strong>
                  <ul style={{ margin: "0.25rem 0", paddingLeft: "1.25rem" }}>
                    {astResult.functions.map((fn, i) => (
                      <li key={i}>
                        <code>{fn.name}</code> (line {fn.line}) — <code>{fn.signature}</code>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {astResult.typesOrClasses.length > 0 && (
                <div style={{ marginBottom: "0.5rem" }}>
                  <strong>Types / Classes ({astResult.typesOrClasses.length}):</strong>
                  <ul style={{ margin: "0.25rem 0", paddingLeft: "1.25rem" }}>
                    {astResult.typesOrClasses.map((t, i) => (
                      <li key={i}>
                        <code>{t.name}</code> ({t.kind})
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {astResult.errorPoints.length > 0 && (
                <div>
                  <strong style={{ color: "#ef4444" }}>Error Points ({astResult.errorPoints.length}):</strong>
                  <ul style={{ margin: "0.25rem 0", paddingLeft: "1.25rem" }}>
                    {astResult.errorPoints.map((err, i) => (
                      <li key={i} style={{ color: "#ef4444" }}>
                        Line {err.line}: {err.signature}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Deterministic Synthesis Report Panel */}
        <div className="card" style={{ padding: "1.25rem" }}>
          <h2 style={{ fontSize: "1.1rem", margin: "0 0 1rem", fontWeight: 600, display: "flex", alignItems: "center", gap: "0.5rem" }}>
            📝 Deterministic Code Synthesis Report
          </h2>

          {report ? (
            <div
              style={{
                background: "var(--bg-subtle)",
                padding: "1rem",
                borderRadius: "6px",
                fontSize: "0.85rem",
                whiteSpace: "pre-wrap",
                fontFamily: "sans-serif",
                maxHeight: "360px",
                overflowY: "auto",
              }}
            >
              {report.summaryMarkdown}
            </div>
          ) : (
            <div style={{ color: "var(--text-muted)", fontSize: "0.9rem" }}>
              Generating deterministic report...
            </div>
          )}
        </div>
      </div>

      {/* Timeline of events */}
      <div className="card" style={{ marginTop: "1.5rem", padding: "1.25rem" }}>
        <h2 style={{ fontSize: "1.1rem", margin: "0 0 1rem", fontWeight: 600 }}>
          📋 Ambient Events Timeline
        </h2>
        {events.length === 0 ? (
          <p style={{ color: "var(--text-muted)", fontSize: "0.9rem" }}>
            No ambient events recorded yet. Run AST extractions above to populate local graph memory.
          </p>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
            {events.map((ev) => (
              <div
                key={ev.id}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  padding: "0.6rem 0.8rem",
                  background: "var(--bg-subtle)",
                  borderRadius: "6px",
                  fontSize: "0.85rem",
                }}
              >
                <div>
                  <span
                    style={{
                      display: "inline-block",
                      padding: "0.15rem 0.4rem",
                      borderRadius: "4px",
                      background: "rgba(236,72,153,0.15)",
                      color: "#ec4899",
                      fontWeight: 600,
                      marginRight: "0.5rem",
                      fontSize: "0.75rem",
                    }}
                  >
                    {ev.eventType.toUpperCase()}
                  </span>
                  <strong>{ev.title}</strong> — {ev.summary}
                </div>
                <div style={{ color: "var(--text-muted)", fontSize: "0.75rem" }}>
                  {new Date(ev.createdAt).toLocaleTimeString()}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
