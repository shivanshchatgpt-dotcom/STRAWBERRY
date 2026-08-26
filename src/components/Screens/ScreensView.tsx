import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { ScreenBlocklistItem, ScreenFrame } from "../../lib/types";

/**
 * 📺 Screen Memory — screenshot capture control + recall grid.
 * pHash-based dedupe, blocklist privacy, FTS5 search on OCR/app/title.
 */
export function ScreensView() {
  const [capturing, setCapturing] = useState(false);
  const [frames, setFrames] = useState<ScreenFrame[] | null>(null);
  const [blocklist, setBlocklist] = useState<ScreenBlocklistItem[]>([]);
  const [query, setQuery] = useState("");
  const [newPattern, setNewPattern] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [f, b] = await Promise.all([
        api.listScreens(60),
        api.listScreenBlocklist(),
      ]);
      setFrames(f);
      setBlocklist(b);
      setError(null);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setFrames([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleCapture = async () => {
    try {
      if (capturing) {
        await api.stopScreenCapture();
        setCapturing(false);
      } else {
        await api.startScreenCapture(null);
        setCapturing(true);
      }
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  const runSearch = async () => {
    const q = query.trim();
    if (!q) {
      await refresh();
      return;
    }
    try {
      const hits = await api.searchScreens(q, 60);
      // Reuse the frames grid by mapping hits into frame-ish rows via get.
      const full = await Promise.all(hits.map((h) => api.getScreenFrame(h.id)));
      setFrames(full.filter((f): f is ScreenFrame => f !== null));
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  const addPattern = async () => {
    const pattern = newPattern.trim();
    if (!pattern) return;
    try {
      await api.addScreenBlocklist(pattern, "manual");
      setNewPattern("");
      await refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  const removePattern = async (id: number) => {
    try {
      await api.removeScreenBlocklist(id);
      await refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  const deleteFrame = async (id: number) => {
    try {
      await api.deleteScreenFrame(id);
      await refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  return (
    <div className="content screens-view">
      <header className="page-head">
        <div>
          <h1>📺 Screen Memory</h1>
          <div className="sub">
            Periodic screenshots with pHash dedupe + privacy blocklist. 100% local.
          </div>
        </div>
        <div className="page-actions">
          <button
            className={`btn ${capturing ? "danger" : "primary"}`}
            onClick={() => void toggleCapture()}
          >
            {capturing ? "⏸ Stop Capture" : "▶ Start Capture (30s)"}
          </button>
          <button className="btn" onClick={() => void refresh()}>
            ↻ Refresh
          </button>
        </div>
      </header>

      {error && <div className="dash-error">⚠️ {error}</div>}

      {/* Search */}
      <div className="quick-row screens-search">
        <input
          className="quick-input"
          placeholder="Search screens (OCR text, app, window title)…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void runSearch()}
        />
        <button className="btn primary" onClick={() => void runSearch()}>
          🔍 Search
        </button>
        {query && (
          <button
            className="btn ghost"
            onClick={() => {
              setQuery("");
              void refresh();
            }}
          >
            Clear
          </button>
        )}
      </div>

      {/* Privacy blocklist */}
      <section className="panel" aria-label="Privacy blocklist">
        <h3 className="panel-title">🛡️ Privacy Blocklist</h3>
        <div className="quick-row">
          <input
            className="quick-input"
            placeholder="Add app/title pattern to never capture (e.g. bank)…"
            value={newPattern}
            onChange={(e) => setNewPattern(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addPattern()}
          />
          <button className="btn" onClick={() => void addPattern()}>
            + Block
          </button>
        </div>
        <div className="blocklist-chips">
          {blocklist.map((b) => (
            <span key={b.id} className="tag-chip blocklist-chip">
              {b.pattern}
              <button
                className="chip-x"
                title="Remove"
                onClick={() => void removePattern(b.id)}
              >
                ✕
              </button>
            </span>
          ))}
          {blocklist.length === 0 && (
            <span className="text-dim">No patterns — everything is captured.</span>
          )}
        </div>
      </section>

      {/* Frames grid */}
      <section className="panel" aria-label="Captured screens">
        <h3 className="panel-title">🖼️ Captured Screens {frames ? `(${frames.length})` : ""}</h3>
        {frames === null ? (
          <div className="loading-block">
            <span className="spinner" /> Loading…
          </div>
        ) : frames.length === 0 ? (
          <div className="text-dim">
            No screens captured yet. Press “Start Capture” — a frame is saved only
            when the screen changes meaningfully (pHash diff ≥ threshold).
          </div>
        ) : (
          <div className="screens-grid">
            {frames.map((f) => (
              <figure key={f.id} className="screen-card" title={f.windowTitle ?? f.appName ?? ""}>
                {f.thumbnailPath ? (
                  <img
                    src={`local-resource://${f.thumbnailPath}`}
                    alt={f.appName ?? "screen"}
                    loading="lazy"
                    onError={(ev) => {
                      (ev.target as HTMLImageElement).style.display = "none";
                    }}
                  />
                ) : (
                  <div className="screen-placeholder">🖥️</div>
                )}
                <figcaption>
                  <span className="screen-app">{f.appName ?? "unknown"}</span>
                  <time className="screen-ts">
                    {new Date(f.ts).toLocaleString()}
                  </time>
                  <button
                    className="icon-btn danger screen-del"
                    title="Delete"
                    onClick={() => void deleteFrame(f.id)}
                  >
                    🗑
                  </button>
                </figcaption>
              </figure>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
