import { useEffect, useState } from "react";
import { api } from "../../lib/api";

/**
 * 📁 Watchers — control the persistent background file watcher.
 *
 * Changes here affect the SAME shared FileWatcherRunner that the
 * background polling thread in lib.rs uses. Watching a directory
 * actually starts delivering events into the EventBus, which the
 * file→memory indexer then turns into memory records.
 */
export function MemoryWatchers() {
  const [paths, setPaths] = useState<string[]>([]);
  const [newPath, setNewPath] = useState("");
  const [privacyOk, setPrivacyOk] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setError(null);
    try {
      const list = await api.watcherList();
      setPaths(list);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => { void refresh(); }, []);

  useEffect(() => {
    if (!newPath.trim()) {
      setPrivacyOk(null);
      return;
    }
    api.watcherCheckPath(newPath.trim())
      .then(setPrivacyOk)
      .catch(() => setPrivacyOk(false));
  }, [newPath]);

  async function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    if (!newPath.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await api.watcherStart(newPath.trim());
      setNewPath("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleStop(path: string) {
    setBusy(true);
    setError(null);
    try {
      await api.watcherStop(path);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="memory-watchers">
      {error && <div className="memory-error-banner">{error}</div>}

      <form className="memory-watcher-form" onSubmit={handleAdd}>
        <h3 className="memory-section-title">Add a watched directory</h3>
        <div className="memory-form-grid">
          <div className="memory-form-field memory-form-field-wide">
            <label>Absolute path</label>
            <input
              type="text"
              value={newPath}
              onChange={(e) => setNewPath(e.target.value)}
              placeholder="/home/user/Documents/MyProject"
            />
          </div>
        </div>
        {privacyOk === false && (
          <div className="memory-watcher-blocked">
            ⚠ This path is blocked by the privacy filter. System paths,
            .ssh/, .gnupg/, node_modules/, .git/, and build artifacts are
            not watched. Choose a user directory.
          </div>
        )}
        {privacyOk === true && (
          <div className="memory-watcher-ok">
            ✓ Path is allowed by the privacy filter.
          </div>
        )}
        <div className="memory-form-actions">
          <button
            className="btn"
            type="submit"
            disabled={busy || !newPath.trim() || privacyOk === false}
          >
            {busy ? "Starting…" : "Start watching"}
          </button>
        </div>
      </form>

      <h3 className="memory-section-title">{paths.length} active watcher{paths.length === 1 ? "" : "s"}</h3>

      {paths.length === 0 ? (
        <div className="memory-empty">
          <h3 className="memory-empty-title">No active watchers</h3>
          <p className="memory-empty-body">
            Start watching a directory above. File events will be turned
            into memory records (Document kind) by the background indexer.
            Privacy-blocked paths are rejected.
          </p>
        </div>
      ) : (
        <ul className="memory-watcher-list">
          {paths.map((p) => (
            <li key={p} className="memory-watcher-item">
              <span className="memory-watcher-path">{p}</span>
              <button
                className="btn btn-small btn-danger"
                onClick={() => void handleStop(p)}
                disabled={busy}
              >
                Stop
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="memory-section-note">
        <h4>How watchers work</h4>
        <ul>
          <li>The watcher is a real OS-level file watcher (notify crate).</li>
          <li>Events flow into a background thread that publishes to the EventBus.</li>
          <li>The file→memory indexer drains the bus and writes Document-kind memories.</li>
          <li>System paths (<code>/etc</code>, <code>/var</code>, …) are blocked.</li>
          <li>Build artifacts (<code>node_modules</code>, <code>/.git/</code>, <code>/target/</code>) are blocked.</li>
          <li>Files matching secret patterns (e.g. <code>.env</code>, <code>id_rsa</code>) are indexed with metadata only — bodies are not stored.</li>
        </ul>
      </div>
    </div>
  );
}
