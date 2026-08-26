import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { snap } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

/**
 * 🧠 Context Recall — one click captures the whole workspace (windows,
 * browser tabs, recent pages). Come back later, hit "Load Previous Work"
 * and Strawberry tells you kaha pe the, kya kar rahe the.
 */
export function ContextRecall() {
  const [latest, setLatest] = useState<snap.WorkSnapshot | null>(null);
  const [detail, setDetail] = useState<snap.WorkSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const showToast = useAppStore((s) => s.showToast);
  const openChat = useAppStore((s) => s.openChat);

  useEffect(() => {
    void api
      .getLatestWorkSnapshot()
      .then((s) => setLatest(s ?? null))
      .catch(() => setLatest(null));
  }, []);

  const capture = useCallback(async () => {
    setBusy(true);
    try {
      const snap = await api.captureWorkSnapshot();
      setLatest(snap);
      setDetail(snap);
      showToast("success", "📸 Snapshot saved — pura workspace capture ho gaya");
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  }, [showToast]);

  const loadPrevious = () => {
    if (latest) setDetail(latest);
  };

  // Group windows by app for the detail view.
  const groups = detail
    ? Object.entries(
        detail.windows.reduce<Record<string, string[]>>((acc, w) => {
          (acc[w.app] ??= []).push(w.title || "(untitled)");
          return acc;
        }, {}),
      ).sort((a, b) => b[1].length - a[1].length)
    : [];

  return (
    <section className="recall" aria-label="Context recall">
      <div className="recall-head">
        <div className="recall-headtext">
          <h3 className="recall-title">🧠 Context Recall</h3>
          <p className="recall-sub">
            Ek click me pura workspace ka snapshot — apps, tabs, notes.
            Agli baar aao aur <b>Load Previous Work</b> dabao: kaha the,
            kya chal raha tha — sab wapas.
          </p>
        </div>
        <div className="recall-actions">
          <button
            className="btn primary recall-capture"
            onClick={() => void capture()}
            disabled={busy}
          >
            {busy ? <span className="spinner" /> : "📸"} {busy ? "Capturing…" : "Snapshot Lo"}
          </button>
          {latest && !detail && (
            <button className="btn" onClick={loadPrevious}>
              📖 Load Previous Work
            </button>
          )}
        </div>
      </div>

      {latest && (
        <div className="recall-last">
          Last snapshot: <b>{fmtTime(latest.createdAt)}</b> •{" "}
          {latest.windows.length} windows
          {latest.browsers.length > 0 &&
            ` • ${latest.browsers.map((b) => `${b.items.length} ${b.browser}`).join(", ")}`}
        </div>
      )}

      {detail && (
        <div className="recall-detail">
          <blockquote className="recall-story">{detail.story}</blockquote>

          <div className="recall-grid">
            <div className="recall-col">
              <h4 className="recall-colhead">🪟 Open Windows ({detail.windows.length})</h4>
              {groups.map(([app, titles]) => (
                <div className="recall-group" key={app}>
                  <span className="recall-app">{app}</span>
                  <ul className="recall-list">
                    {titles.slice(0, 6).map((t, i) => (
                      <li key={i}>{t}</li>
                    ))}
                  </ul>
                </div>
              ))}
              {groups.length === 0 && (
                <p className="recall-empty">Koi window nahi mili.</p>
              )}
            </div>

            <div className="recall-col">
              {detail.browsers.map((b) => (
                <div key={b.browser + b.kind} className="recall-browser">
                  <h4 className="recall-colhead">
                    {b.kind === "tabs" ? "🌐" : "🕘"} {b.browser} {b.kind === "tabs" ? "tabs" : "recent"} ({b.items.length})
                  </h4>
                  <ul className="recall-list">
                    {b.items.slice(0, 8).map((t, i) => (
                      <li key={i} title={t.url}>
                        {t.title}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
              {detail.clipboardHint && (
                <div className="recall-clip">
                  <h4 className="recall-colhead">📋 Clipboard tha</h4>
                  <code>{detail.clipboardHint}</code>
                </div>
              )}
              {detail.browsers.length === 0 && !detail.clipboardHint && (
                <p className="recall-empty">Browser/clipboard context nahi mila.</p>
              )}
            </div>
          </div>

          {detail.relatedNotes.length > 0 && (
            <div className="recall-related">
              <h4 className="recall-colhead">🔗 Kyu — tumhare hi purane notes jo is context se judte hain</h4>
              <div className="recall-chips">
                {detail.relatedNotes.map((n) => (
                  <button
                    key={n.chatId}
                    className="recall-chip"
                    title="Open this note"
                    onClick={() => void openChat(n.chatId)}
                  >
                    📄 {n.title}
                  </button>
                ))}
              </div>
            </div>
          )}

          <button className="btn recall-close" onClick={() => setDetail(null)}>
            Close
          </button>
        </div>
      )}
    </section>
  );
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const today = new Date().toDateString() === d.toDateString();
  return today
    ? d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleString(undefined, {
        day: "numeric",
        month: "short",
        hour: "2-digit",
        minute: "2-digit",
      });
}
