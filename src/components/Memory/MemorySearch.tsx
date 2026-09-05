import { useEffect, useRef, useState } from "react";
import { api, type SearchPage } from "../../lib/api";
import type { MemoryNav } from "./nav";

/**
 * 🔎 Memory Search — searches the unified_memories table via the
 * `memory_search` Tauri command. Debounced, paginated (load more),
 * no frontend mock — every result comes from SQLite/FTS.
 */
export function MemorySearch({ onNavigate }: { onNavigate: MemoryNav }) {
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"" | "working" | "episodic" | "semantic" | "project" | "procedural" | "credential" | "image" | "document" | "block" | "generic">("");
  const [project, setProject] = useState("");
  const [app, setApp] = useState("");
  const [page, setPage] = useState<SearchPage | null>(null);
  const [accumulated, setAccumulated] = useState<SearchPage["hits"]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const PAGE_SIZE = 25;
  // Latest query ref so debounce + load-more agree.
  const queryRef = useRef({ text: "", kind: "", project: "", app: "" });
  queryRef.current = { text: query, kind, project, app };

  // Debounce search: 250ms after the user stops typing.
  useEffect(() => {
    const handle = setTimeout(() => { void runSearch(0, true); }, 250);
    return () => clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, kind, project, app]);

  async function runSearch(off: number, reset: boolean) {
    setError(null);
    setLoading(true);
    try {
      const r = queryRef.current;
      const result = await api.memorySearch({
        text: r.text,
        kind: r.kind || undefined,
        project: r.project || undefined,
        app: r.app || undefined,
        limit: PAGE_SIZE,
        offset: off,
      });
      setPage(result);
      setOffset(off);
      if (reset) {
        setAccumulated(result.hits);
      } else {
        setAccumulated((prev) => [...prev, ...result.hits]);
      }
    } catch (e) {
      setError(String(e));
      if (reset) {
        setPage(null);
        setAccumulated([]);
      }
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="memory-search">
      <div className="memory-search-bar">
        <input
          className="memory-search-input"
          type="text"
          placeholder="Search memories — text, tags, content, projects…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoFocus
        />
        <select
          className="memory-search-select"
          value={kind}
          onChange={(e) => setKind(e.target.value as typeof kind)}
        >
          <option value="">All types</option>
          <option value="working">working</option>
          <option value="episodic">episodic</option>
          <option value="semantic">semantic</option>
          <option value="project">project</option>
          <option value="procedural">procedural</option>
          <option value="credential">credential</option>
          <option value="image">image</option>
          <option value="document">document</option>
          <option value="block">block</option>
          <option value="generic">generic</option>
        </select>
        <input
          className="memory-search-input memory-search-input-narrow"
          type="text"
          placeholder="Project"
          value={project}
          onChange={(e) => setProject(e.target.value)}
        />
        <input
          className="memory-search-input memory-search-input-narrow"
          type="text"
          placeholder="App"
          value={app}
          onChange={(e) => setApp(e.target.value)}
        />
        <button
          className="btn"
          onClick={() => void runSearch(0, true)}
          disabled={loading}
        >
          {loading ? "…" : "Search"}
        </button>
      </div>

      {page && (
        <div className="memory-search-stats">
          {accumulated.length} of {page.total} matches
        </div>
      )}

      {error && (
        <div className="memory-search-error">
          <strong>Search failed.</strong> {error}
        </div>
      )}

      {!loading && accumulated.length === 0 && query.trim() !== "" && !error && (
        <div className="memory-empty">
          <h3 className="memory-empty-title">No results</h3>
          <p className="memory-empty-body">
            Try a different query or remove filters.
          </p>
        </div>
      )}

      {!loading && accumulated.length === 0 && query.trim() === "" && !error && (
        <div className="memory-empty">
          <h3 className="memory-empty-title">Search the memory graph</h3>
          <p className="memory-empty-body">
            Enter text to find memories by title, content, tags, project,
            or source application. Secret credential bodies are never
            included in normal search.
          </p>
        </div>
      )}

      <ul className="memory-search-results">
        {accumulated.map((h) => (
          <li
            key={h.memory.id}
            className="memory-search-item"
            onClick={() => {
              void api.memoryRecordView(h.memory.id).catch(() => undefined);
              onNavigate.goDetail(h.memory.id);
            }}
          >
            <div className="memory-search-item-head">
              <span className={`memory-pill memory-pill-${h.memory.kind}`}>
                {h.memory.kind}
              </span>
              <span className="memory-search-item-title">{h.memory.title}</span>
              <span className="memory-search-item-score">
                {h.score.toFixed(2)}
              </span>
            </div>
            <div className="memory-search-item-content">
              {h.memory.content.slice(0, 240)}
              {h.memory.content.length > 240 ? "…" : ""}
            </div>
            <div className="memory-search-item-meta">
              {h.memory.projectId && (
                <span className="memory-meta-tag">📁 {h.memory.projectId}</span>
              )}
              {h.memory.sourceApplication && (
                <span className="memory-meta-tag">🖥 {h.memory.sourceApplication}</span>
              )}
              {h.memory.sourceFile && (
                <span className="memory-meta-tag">📄 {truncatePath(h.memory.sourceFile, 40)}</span>
              )}
              {h.memory.tags.map((t) => (
                <span key={t} className="memory-meta-tag">#{t}</span>
              ))}
              <span className="memory-meta-tag">
                views {h.memory.viewCount} · uses {h.memory.useCount}
              </span>
              {h.memory.lastUsedAtMs && (
                <span className="memory-meta-tag">
                  used {formatRelative(h.memory.lastUsedAtMs)}
                </span>
              )}
            </div>
            {h.matchedVia.length > 0 && (
              <div className="memory-search-item-matched">
                matched: {h.matchedVia.join(", ")}
              </div>
            )}
          </li>
        ))}
      </ul>

      {page && page.hasMore && (
        <div className="memory-search-loadmore">
          <button
            className="btn btn-ghost"
            onClick={() => void runSearch(offset + PAGE_SIZE, false)}
            disabled={loading}
          >
            {loading ? "Loading…" : `Load more (${page.total - accumulated.length} remaining)`}
          </button>
        </div>
      )}
    </div>
  );
}

function truncatePath(p: string, max: number): string {
  if (p.length <= max) return p;
  return "…" + p.slice(p.length - max + 1);
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
