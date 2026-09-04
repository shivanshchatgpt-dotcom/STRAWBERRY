import { useState } from "react";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

/**
 * 📖 Export My Story — buildathon-ready narrative from local data.
 * Moved from the dashboard bottom into its own left-nav view.
 */
export function StoryExportView() {
  const [story, setStory] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [repoPath, setRepoPath] = useState("");
  const [error, setError] = useState<string | null>(null);

  const generate = async () => {
    setBusy(true);
    setError(null);
    try {
      const md = await api.exportMyStory(repoPath.trim() || null, 14);
      setStory(md);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setStory("Story generation failed.");
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    if (story) await useAppStore.getState().copyText(story, "My Story");
  };

  return (
    <div className="content dashboard">
      <header className="dash-head">
        <div>
          <h1 className="dash-title">📖 My Story</h1>
          <div className="meta-line">100% local · zero LLM</div>
        </div>
        <button className="btn" onClick={() => void copy()} disabled={!story}>
          📋 Copy
        </button>
      </header>

      {error && <div className="dash-error">⚠️ {error}</div>}

      <section className="panel" aria-label="Story generator">
        <h3 className="panel-title">Export My Story</h3>
        <p className="text-dim" style={{ fontSize: 12.5, margin: "4px 0 10px" }}>
          Chats, captures, tasks, habits (+ optional git commit timeline) se ek
          ready-to-share narrative banti hai — sab kuch local, koi LLM nahi.
        </p>
        <div className="quick-row">
          <input
            className="quick-input"
            placeholder="Optional: path to a git repo for commit timeline…"
            value={repoPath}
            onChange={(e) => setRepoPath(e.target.value)}
          />
          <button className="btn primary" disabled={busy} onClick={() => void generate()}>
            {busy ? <span className="spinner" /> : null} Generate
          </button>
        </div>
      </section>

      {story && <pre className="pre-block brief-text story-output">{story}</pre>}
    </div>
  );
}
