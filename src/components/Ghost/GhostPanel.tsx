import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../lib/api";
import type { ghost } from "../../lib/types";
import { useAppStore } from "../../store/appStore";

const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const INSIGHT_ICONS: Record<string, string> = {
  serendipity: "🌀",
  pattern: "📊",
  resurface: "💤",
  cluster: "🧠",
  warning: "⚠️",
  achievement: "🎯",
};

const KIND_COLORS: Record<string, string> = {
  chat: "#34d399",
  folder: "#60a5fa",
  root: "#fb7185",
  tag: "#fbbf24",
};

interface Position {
  x: number;
  y: number;
}

function colorFor(kind: string): string {
  return KIND_COLORS[kind] || "#64748b";
}

export function GhostPanel() {
  const [snap, setSnap] = useState<ghost.GhostSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"graph" | "heatmap" | "stats">("graph");
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [insightOffset, setInsightOffset] = useState(0);
  const graphCanvasRef = useRef<HTMLCanvasElement>(null);
  const heatmapCanvasRef = useRef<HTMLCanvasElement>(null);
  const animFrameRef = useRef<number>(0);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      const data = await api.ghostGetSnapshot();
      setSnap(data);
      setError(null);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const unlisten = useAppStore.subscribe((state, prev) => {
      if (state.currentChatId !== prev.currentChatId && state.currentChatId) {
        void api.ghostRecordEvent("open_chat", state.currentChatId, "chat", 0);
      }
      if (state.currentRootId !== prev.currentRootId && state.currentRootId) {
        void api.ghostRecordEvent("open_root", state.currentRootId, "root", 0);
      }
    });
    return unlisten;
  }, []);

  useEffect(() => {
    const t = setInterval(() => void refresh(), 30_000);
    return () => clearInterval(t);
  }, [refresh]);

  const positions = useMemo(() => {
    if (!snap) return {} as Record<string, Position>;
    const pos: Record<string, Position> = {};
    const nodes = snap.graph.nodes;
    const cw = 600;
    const ch = 400;
    const cx = cw / 2;
    const cy = ch / 2;
    const rootNodes: ghost.GraphNode[] = nodes.filter(n => n.kind === "root");
    const folderNodes: ghost.GraphNode[] = nodes.filter(n => n.kind === "folder");
    const chatNodes: ghost.GraphNode[] = nodes.filter(n => n.kind === "chat");
    const tagNodes: ghost.GraphNode[] = nodes.filter(n => n.kind === "tag");

    rootNodes.forEach((n: ghost.GraphNode, i: number) => {
      const angle = (i / Math.max(1, rootNodes.length)) * Math.PI * 2;
      pos[n.id] = { x: cx + Math.cos(angle) * 80, y: cy + Math.sin(angle) * 80 };
    });

    folderNodes.forEach((n: ghost.GraphNode, i: number) => {
      const parentRoot = n.id.replace("folder:", "root:");
      const parentPos = pos[parentRoot] || { x: cx, y: cy };
      const angle = (i / Math.max(1, folderNodes.length)) * Math.PI * 2;
      pos[n.id] = { x: parentPos.x + Math.cos(angle) * 140, y: parentPos.y + Math.sin(angle) * 140 };
    });

    chatNodes.forEach((n: ghost.GraphNode, i: number) => {
      const parent = n.id.replace("chat:", "folder:");
      const parentPos = pos[parent] || { x: cx, y: cy };
      const angle = (i / Math.max(1, chatNodes.length)) * Math.PI * 2;
      const r = 100 + ((i * 37) % 80);
      pos[n.id] = { x: parentPos.x + Math.cos(angle) * r, y: parentPos.y + Math.sin(angle) * r };
    });

    tagNodes.forEach((n: ghost.GraphNode, i: number) => {
      const angle = (i / Math.max(1, tagNodes.length)) * Math.PI * 2;
      pos[n.id] = { x: cx + Math.cos(angle) * 260, y: cy + Math.sin(angle) * 260 };
    });

    return pos;
  }, [snap?.graph.nodes]);

  // Animation loop
  useEffect(() => {
    if (!snap) return;
    const render = () => {
      drawGraph();
      drawHeatmap();
      animFrameRef.current = requestAnimationFrame(render);
    };
    render();
    return () => cancelAnimationFrame(animFrameRef.current);
  }, [snap, positions, hoveredNode]);

  function drawGraph() {
    const canvas = graphCanvasRef.current;
    if (!canvas || !snap) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const cw = canvas.clientWidth;
    const ch = canvas.clientHeight;
    canvas.width = cw * dpr;
    canvas.height = ch * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cw, ch);

    const time = Date.now() / 1000;

    snap.graph.edges.forEach((e: ghost.GraphEdge) => {
      const a = positions[e.sourceId];
      const b = positions[e.targetId];
      if (!a || !b) return;
      const opacity = 0.25 * e.weight;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.strokeStyle = `rgba(148,163,184,${opacity})`;
      ctx.lineWidth = Math.max(0.5, 2 * e.weight);
      ctx.stroke();
    });

    snap.graph.nodes.forEach((n: ghost.GraphNode) => {
      const p = positions[n.id];
      if (!p) return;
      const color = n.color || colorFor(n.kind);
      const weight = n.weight;
      const baseR = 4 + Math.min(8, weight);
      const pulse = 1 + 0.15 * Math.sin(time * 2 + (n.id.charCodeAt(0) % 10));
      const r = baseR * pulse;
      const isHovered = hoveredNode === n.id;

      if (isHovered) {
        ctx.beginPath();
        ctx.arc(p.x, p.y, r + 6, 0, Math.PI * 2);
        ctx.fillStyle = `${color}33`;
        ctx.fill();
      }

      ctx.beginPath();
      ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();

      if (isHovered) {
        ctx.beginPath();
        ctx.arc(p.x, p.y, r + 3, 0, Math.PI * 2);
        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.stroke();
      }

      if (isHovered || n.kind === "root" || n.kind === "folder") {
        ctx.font = "11px ui-sans-serif,system-ui";
        ctx.fillStyle = "rgba(255,255,255,0.85)";
        ctx.textAlign = "center";
        ctx.fillText(n.label.slice(0, 20), p.x, p.y - r - 6);
      }
    });
  }

  function drawHeatmap() {
    const canvas = heatmapCanvasRef.current;
    if (!canvas || !snap) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const cw = canvas.clientWidth;
    const ch = canvas.clientHeight;
    canvas.width = cw * dpr;
    canvas.height = ch * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cw, ch);

    const cells = snap.heatmap;
    const maxCount = Math.max(1, ...cells.map(c => c.count));
    const cellW = cw / 24;
    const cellH = (ch - 24) / 7;
    const topPad = 24;

    cells.forEach((cell: ghost.AttentionCell) => {
      const x = cell.hour * cellW;
      const y = topPad + cell.day * cellH;
      const intensity = cell.count / maxCount;
      const light = 90 - intensity * 60;
      const opacity = 0.3 + intensity * 0.7;

      ctx.fillStyle = `hsla(160, 70%, ${light}%, ${opacity})`;
      ctx.fillRect(x + 1, y + 1, cellW - 2, cellH - 2);

      if (cell.count > 0) {
        ctx.globalAlpha = 0.3;
        ctx.fillStyle = "hsl(160, 80%, 40%)";
        ctx.fillRect(x + 1, y + 1, cellW - 2, cellH - 2);
        ctx.globalAlpha = 1;
      }
    });

    ctx.font = "10px ui-sans-serif,system-ui";
    ctx.fillStyle = "rgba(148,163,184,0.6)";
    ctx.textAlign = "center";
    for (let h = 0; h < 24; h++) {
      ctx.fillText(h === 0 ? "00" : h < 10 ? `0${h}` : `${h}`, h * cellW + cellW / 2, 14);
    }

    ctx.textAlign = "right";
    DAY_LABELS.forEach((d, i) => {
      ctx.fillText(d, 4, topPad + i * cellH + cellH / 2 + 3);
    });
  }

  function truncate(s: string, n: number) {
    return s.length > n ? s.slice(0, n) + "…" : s;
  }

  if (error) {
    return (
      <div className="panel" style={{ borderColor: "#ef444440" }}>
        <h3 className="panel-title">👻 Ghost</h3>
        <p style={{ color: "#ef4444", fontSize: 13 }}>{error}</p>
        <button className="btn primary" onClick={() => void refresh()} disabled={busy}>
          {busy ? <span className="spinner" /> : "Retry"}
        </button>
      </div>
    );
  }

  if (!snap) {
    return (
      <div className="panel">
        <h3 className="panel-title">👻 Ghost</h3>
        <div className="loading-block"><span className="spinner" /> Ghost is waking up…</div>
      </div>
    );
  }

  const { stats, insights, recentEvents } = snap;

  return (
    <section className="panel ghost-panel" aria-label="The Strawberry Ghost">
      <header className="ghost-header">
        <h3 className="panel-title">👻 The Strawberry Ghost</h3>
        <div className="ghost-status">
          <span className="ghost-pulse" aria-label="Ghost is active" />
          <span className="text-dim" style={{ fontSize: 12 }}>
            {stats.totalEvents.toLocaleString()} events · {stats.graphNodes} nodes · {stats.graphEdges} edges
          </span>
        </div>
      </header>

      <div className="ghost-tabs">
        <button
          className={`ghost-tab ${activeTab === "graph" ? "active" : ""}`}
          onClick={() => setActiveTab("graph")}
        >
          🕸️ Knowledge Graph
        </button>
        <button
          className={`ghost-tab ${activeTab === "heatmap" ? "active" : ""}`}
          onClick={() => setActiveTab("heatmap")}
        >
          🔥 Attention Heatmap
        </button>
        <button
          className={`ghost-tab ${activeTab === "stats" ? "active" : ""}`}
          onClick={() => setActiveTab("stats")}
        >
          📈 Stats & Insights
        </button>
      </div>

      {activeTab === "graph" && (
        <div className="ghost-graph-wrapper">
          <canvas
            ref={graphCanvasRef}
            className="ghost-graph"
            onMouseMove={e => {
              const rect = e.currentTarget.getBoundingClientRect();
              const x = e.clientX - rect.left;
              const y = e.clientY - rect.top;
              let closest: string | null = null;
              let minDist = 20;
              Object.entries(positions).forEach(([id, p]: [string, Position]) => {
                const d = Math.hypot(x - p.x, y - p.y);
                if (d < minDist) {
                  minDist = d;
                  closest = id;
                }
              });
              setHoveredNode(closest);
            }}
            onMouseLeave={() => setHoveredNode(null)}
          />
        </div>
      )}

      {activeTab === "heatmap" && (
        <div className="ghost-heatmap-wrapper">
          <div className="heatmap-legend">
            <span className="text-dim" style={{ fontSize: 11 }}>Last 90 days · Mon–Sun × 0–23h</span>
            <div className="legend-bar">
              {[0, 0.25, 0.5, 0.75, 1].map(v => (
                <div
                  key={v}
                  style={{
                    width: 16,
                    height: 10,
                    background: `hsl(160, 70%, ${90 - v * 60}%)`,
                    opacity: 0.3 + v * 0.7,
                    borderRadius: 2,
                  }}
                />
              ))}
            </div>
          </div>
          <canvas ref={heatmapCanvasRef} className="ghost-heatmap" />
        </div>
      )}

      {activeTab === "stats" && (
        <div className="ghost-stats">
          <div className="stats-grid">
            <div className="stat-card">
              <div className="stat-value">{stats.totalEvents.toLocaleString()}</div>
              <div className="stat-label">Total events recorded</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{stats.streakDays}</div>
              <div className="stat-label">Activity streak (days)</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{stats.graphNodes}</div>
              <div className="stat-label">Knowledge nodes</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{stats.graphEdges}</div>
              <div className="stat-label">Connections</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{stats.peakHour !== null ? `${stats.peakHour}:00` : "—"}</div>
              <div className="stat-label">Peak activity hour</div>
            </div>
            <div className="stat-card">
              <div className="stat-value">{DAY_LABELS[stats.peakDay ?? 0]}</div>
              <div className="stat-label">Peak day</div>
            </div>
          </div>

          <div className="stats-section">
            <h4 className="panel-title" style={{ fontSize: 13, marginBottom: 8 }}>🏆 Top Visited</h4>
            <div className="bar-list">
              {stats.mostVisited.slice(0, 8).map((item: [string, string, number]) => {
                const [id, label, count] = item;
                return (
                  <div key={id} className="bar-row">
                    <span className="bar-label" title={label}>{truncate(label, 28)}</span>
                    <div className="bar-track" style={{ flex: 1 }}>
                      <div
                        className="bar-fill is-berry"
                        style={{ width: `${Math.min(100, ((count || 0) / ((stats.mostVisited[0]?.[2] || 1))) * 100)}%` }}
                      />
                    </div>
                    <span className="bar-count">{count}</span>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="stats-section">
            <h4 className="panel-title" style={{ fontSize: 13, marginBottom: 8 }}>🏷️ Top Tags</h4>
            <div className="tag-cloud">
              {stats.topTags.slice(0, 12).map((item: [string, number]) => {
                const [tag, count] = item;
                return (
                  <span
                    key={tag}
                    className="tag-chip"
                    style={{
                      fontSize: Math.max(11, 11 + Math.min(4, count / 2)),
                      opacity: 0.5 + Math.min(0.5, count / 10),
                    }}
                  >
                    #{tag} <span style={{ opacity: 0.6 }}>{count}</span>
                  </span>
                );
              })}
            </div>
          </div>

          {insights.length > 0 && (
            <div className="stats-section">
              <h4 className="panel-title" style={{ fontSize: 13, marginBottom: 8 }}>
                💡 Insights <span className="text-dim">({insights.length})</span>
              </h4>
              <div className="insight-list" style={{ maxHeight: 300, overflowY: "auto" }}>
                {insights.slice(insightOffset, insightOffset + 5).map((ins: ghost.GhostInsight) => (
                  <div
                    key={ins.id}
                    className="insight-card"
                    onClick={() => void api.ghostMarkSeen(ins.id)}
                    style={{ animationDelay: `${ins.id * 30}ms` }}
                  >
                    <span className="insight-icon">{INSIGHT_ICONS[ins.kind] || "💡"}</span>
                    <div className="insight-content">
                      <div className="insight-title">{ins.title}</div>
                      <div className="insight-body">{ins.body}</div>
                    </div>
                    <span className="insight-score" style={{ color: `hsl(${120 * ins.score}, 70%, 40%)` }}>
                      {Math.round(ins.score * 100)}%
                    </span>
                  </div>
                ))}
              </div>
              {insights.length > 5 && (
                <div className="insight-pager">
                  <button className="btn" onClick={() => setInsightOffset(Math.max(0, insightOffset - 5))} disabled={insightOffset === 0}>
                    ← Earlier
                  </button>
                  <span className="text-dim" style={{ fontSize: 11 }}>
                    {insightOffset + 1}–{Math.min(insightOffset + 5, insights.length)} of {insights.length}
                  </span>
                  <button className="btn" onClick={() => setInsightOffset(insightOffset + 5)} disabled={insightOffset + 5 >= insights.length}>
                    Later →
                  </button>
                </div>
              )}
            </div>
          )}

          <div className="stats-section">
            <h4 className="panel-title" style={{ fontSize: 13, marginBottom: 8 }}>🕰️ Recent Activity</h4>
            <div className="event-timeline">
              {recentEvents.slice(0, 10).map((ev: ghost.GhostEvent) => (
                <div key={ev.id} className="event-item">
                  <span className="event-time">
                    {new Date(ev.createdAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                  </span>
                  <span className={`event-badge event-${ev.eventType.replace("_", "-")}`}>
                    {ev.eventType.replace("_", " ")}
                  </span>
                  {ev.sourceId && <span className="event-source" title={ev.sourceId}>{ev.sourceId.slice(0, 16)}…</span>}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      <div className="ghost-actions">
        <button className="btn" onClick={() => void api.ghostRebuildGraph()}>
          🔄 Rebuild Graph
        </button>
        <button className="btn" onClick={() => void api.ghostRegenerateInsights()}>
          🔮 New Insights
        </button>
      </div>
    </section>
  );
}
