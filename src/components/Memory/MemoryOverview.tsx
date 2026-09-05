import { useEffect, useState } from "react";
import { api, type SearchHit, type RuntimeSnapshot, type ImageAsset } from "../../lib/api";
import type { MemoryNav } from "./nav";

interface Stats {
  total: number;
  byKind: { kind: string; count: number }[];
  byPrivacy: { level: string; count: number }[];
  recentlyViewed: SearchHit[];
  recentlyCreated: SearchHit[];
  recentImages: ImageAsset[];
  relations: number;
  runtime: RuntimeSnapshot | null;
}

/**
 * 🧠 Memory Overview — real stats from the backend.
 */
export function MemoryOverview({ onNavigate }: { onNavigate: MemoryNav }) {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    async function load() {
      try {
        const total = await api.memoryCount();
        const allPage = await api.memorySearch({ text: "", limit: 200 });
        const all = allPage.hits;
        const byKind: Record<string, number> = {};
        const byPrivacy: Record<string, number> = {};
        for (const hit of all) {
          byKind[hit.memory.kind] = (byKind[hit.memory.kind] ?? 0) + 1;
          byPrivacy[hit.memory.privacyLevel] =
            (byPrivacy[hit.memory.privacyLevel] ?? 0) + 1;
        }
        const recentlyViewed = all
          .filter((h) => h.memory.lastViewedAtMs != null)
          .sort((a, b) => (b.memory.lastViewedAtMs ?? 0) - (a.memory.lastViewedAtMs ?? 0))
          .slice(0, 5);
        const recentlyCreated = [...all]
          .sort((a, b) => b.memory.createdAtMs - a.memory.createdAtMs)
          .slice(0, 5);
        let recentImages: ImageAsset[] = [];
        try {
          recentImages = await api.imageList(6);
        } catch {
          /* images module may have no rows yet */
        }
        let runtime: RuntimeSnapshot | null = null;
        try {
          runtime = await api.autonomyGetStats();
        } catch {
          /* autonomy is optional */
        }
        let relations = 0;
        for (const hit of all.slice(0, 30)) {
          try {
            const rels = await api.memoryListRelationships(hit.memory.id);
            relations += rels.length;
          } catch { /* */ }
        }
        if (cancelled) return;
        setStats({
          total,
          byKind: Object.entries(byKind).map(([kind, count]) => ({ kind, count })),
          byPrivacy: Object.entries(byPrivacy).map(([level, count]) => ({ level, count })),
          recentlyViewed,
          recentlyCreated,
          recentImages,
          relations,
          runtime,
        });
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => { cancelled = true; };
  }, []);

  if (loading) {
    return (
      <div className="memory-overview-loading">
        <p>Loading memory statistics…</p>
      </div>
    );
  }
  if (error) {
    return (
      <div className="memory-overview-error">
        <h3>Could not load memory</h3>
        <pre className="memory-error-msg">{error}</pre>
      </div>
    );
  }
  if (!stats || stats.total === 0) {
    return (
      <div className="memory-empty">
        <h2 className="memory-empty-title">No memories yet</h2>
        <p className="memory-empty-body">
          Create your first memory to start building Strawberry's knowledge.
        </p>
        <div className="memory-empty-actions">
          <button className="btn" onClick={() => onNavigate.goCreate()}>
            + Create Memory
          </button>
          <button
            className="btn btn-ghost"
            onClick={() => onNavigate.goWatchers()}
          >
            📁 Watch a directory
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="memory-overview">
      <h2 className="memory-section-title">
        {stats.total} {stats.total === 1 ? "memory" : "memories"}
      </h2>

      <div className="memory-stats-grid">
        <StatCard
          label="Total"
          value={stats.total.toString()}
          sub="across all types"
        />
        <StatCard
          label="Relationships"
          value={stats.relations.toString()}
          sub="typed edges"
        />
        <StatCard
          label="Images"
          value={stats.recentImages.length.toString()}
          sub="stored"
        />
        <StatCard
          label="Autonomy"
          value={stats.runtime?.mode ?? "—"}
          sub={stats.runtime ? `cycle ${stats.runtime.stats.cyclesTotal}` : "disabled"}
        />
      </div>

      <div className="memory-overview-row">
        <div className="memory-overview-col">
          <h3 className="memory-col-title">By type</h3>
          <div className="memory-kind-list">
            {stats.byKind.length === 0 && <p className="memory-muted">No data</p>}
            {stats.byKind.map((k) => (
              <div key={k.kind} className="memory-kind-row">
                <span className={`memory-pill memory-pill-${k.kind}`}>
                  {k.kind}
                </span>
                <span className="memory-kind-count">{k.count}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="memory-overview-col">
          <h3 className="memory-col-title">By privacy level</h3>
          <div className="memory-kind-list">
            {stats.byPrivacy.length === 0 && <p className="memory-muted">No data</p>}
            {stats.byPrivacy.map((p) => (
              <div key={p.level} className="memory-kind-row">
                <span className={`memory-pill memory-pill-privacy-${p.level}`}>
                  {p.level}
                </span>
                <span className="memory-kind-count">{p.count}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="memory-overview-row">
        <div className="memory-overview-col">
          <h3 className="memory-col-title">Recently viewed</h3>
          {stats.recentlyViewed.length === 0 ? (
            <p className="memory-muted">No views yet</p>
          ) : (
            <ul className="memory-list">
              {stats.recentlyViewed.map((h) => (
                <li
                  key={h.memory.id}
                  className="memory-list-item"
                  onClick={() => onNavigate.goDetail(h.memory.id)}
                >
                  <span className="memory-list-kind">{h.memory.kind}</span>
                  <span className="memory-list-title">{h.memory.title}</span>
                  <span className="memory-list-time">
                    {formatRelative(h.memory.lastViewedAtMs)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="memory-overview-col">
          <h3 className="memory-col-title">Recently created</h3>
          {stats.recentlyCreated.length === 0 ? (
            <p className="memory-muted">No creations yet</p>
          ) : (
            <ul className="memory-list">
              {stats.recentlyCreated.map((h) => (
                <li
                  key={h.memory.id}
                  className="memory-list-item"
                  onClick={() => onNavigate.goDetail(h.memory.id)}
                >
                  <span className="memory-list-kind">{h.memory.kind}</span>
                  <span className="memory-list-title">{h.memory.title}</span>
                  <span className="memory-list-time">
                    {formatRelative(h.memory.createdAtMs)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <div className="memory-overview-row">
        <div className="memory-overview-col memory-overview-col-wide">
          <h3 className="memory-col-title">Recent images</h3>
          {stats.recentImages.length === 0 ? (
            <p className="memory-muted">
              No images yet — go to the Images tab to add one.
            </p>
          ) : (
            <div className="memory-image-grid">
              {stats.recentImages.map((img) => (
                <div
                  key={img.id}
                  className="memory-image-thumb"
                  onClick={() => onNavigate.goImages()}
                  title={img.originalPath}
                >
                  <div className="memory-image-thumb-icon">🖼️</div>
                  <div className="memory-image-thumb-meta">
                    <span className="memory-image-thumb-name">
                      {img.caption ?? img.originalPath.split("/").pop()}
                    </span>
                    <span className={`memory-pill memory-pill-ocr-${img.ocrStatus}`}>
                      {img.ocrStatus}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function StatCard({
  label,
  value,
  sub,
}: {
  label: string;
  value: string;
  sub: string;
}) {
  return (
    <div className="memory-stat-card">
      <div className="memory-stat-label">{label}</div>
      <div className="memory-stat-value">{value}</div>
      <div className="memory-stat-sub">{sub}</div>
    </div>
  );
}

function formatRelative(ms: number | null | undefined): string {
  if (!ms) return "—";
  const delta = Date.now() - ms;
  if (delta < 0) return "just now";
  const sec = Math.floor(delta / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const days = Math.floor(hr / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(ms).toLocaleDateString();
}
