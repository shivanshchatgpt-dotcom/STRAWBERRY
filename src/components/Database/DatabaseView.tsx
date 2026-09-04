import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { DbOverviewData } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

/**
 * 🗄️ Database view — one place to SEE everything saved.
 *
 * Ctrl+C captures (popup → save) land in the "🍓 Captures" root; this view
 * surfaces them with live counts for every table so nothing ever feels
 * "saved but invisible" again.
 */

const KIND_EMOJI: Record<string, string> = {
  note: "📝",
  code: "💻",
  error: "❌",
  url: "🔗",
};

const fmtSize = (b: number) =>
  b >= 1048576 ? `${(b / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`;

const fmtTime = (iso: string) => {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso.slice(0, 16);
  }
};

export function DatabaseView() {
  const [data, setData] = useState<DbOverviewData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const openChat = useAppStore((s) => s.openChat);

  const refresh = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setData(await api.getDbOverview());
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (error) {
    return (
      <div className="content dashboard">
        <div className="dash-error">⚠️ {error}</div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="content dashboard">
        <div className="loading-block"><span className="spinner" /> Loading database…</div>
      </div>
    );
  }

  const o = data.overview;
  const recent = data.recent;

  const tiles: Array<{ emoji: string; label: string; value: number; sub?: string }> = [
    { emoji: "🌳", label: "Roots", value: o.roots },
    { emoji: "📁", label: "Folders", value: o.folders },
    { emoji: "💬", label: "Chats", value: o.chats },
    { emoji: "📥", label: "Captures", value: o.captures, sub: "Ctrl+C saves" },
    { emoji: "📋", label: "Open tasks", value: o.todosOpen },
    { emoji: "✅", label: "Done tasks", value: o.todosDone },
    { emoji: "🔥", label: "Habits", value: o.habits },
    { emoji: "📅", label: "Events", value: o.events },
    { emoji: "🎯", label: "Alpha hits", value: o.alphaCandidates },
    { emoji: "👻", label: "Insights", value: o.insights },
  ];

  const captureKinds: Array<{ label: string; emoji: string; value: number }> = [
    { label: "Notes", emoji: "📝", value: o.captureNotes },
    { label: "Code", emoji: "💻", value: o.captureCode },
    { label: "Errors", emoji: "❌", value: o.captureErrors },
    { label: "Links", emoji: "🔗", value: o.captureUrls },
  ];

  return (
    <div className="content dashboard">
      <header className="dash-head">
        <div>
          <h1 className="dash-title">🗄️ Database</h1>
          <div className="meta-line">
            app.db · {fmtSize(o.dbSizeBytes)} · everything Strawberry has saved
          </div>
        </div>
        <button className="btn" onClick={() => void refresh()} disabled={busy}>
          {busy ? <span className="spinner" /> : null} ↻ Refresh
        </button>
      </header>

      {/* ---- capture kind breakdown ---- */}
      <section className="panel" aria-label="Capture breakdown">
        <h3 className="panel-title">📥 Ctrl+C Captures</h3>
        <p className="text-dim" style={{ fontSize: 12.5, margin: "4px 0 10px" }}>
          Jo bhi copy karke popup me save kiya — sab yahan "🍓 Captures" root me
          hota hai, searchable bhi.
        </p>
        <div className="health-grid">
          {captureKinds.map((k) => (
            <div key={k.label} className="stat-box">
              <div className="stat-value">{k.emoji} {k.value}</div>
              <div className="stat-label">{k.label}</div>
            </div>
          ))}
        </div>
      </section>

      {/* ---- all counts ---- */}
      <section className="panel" aria-label="Table counts">
        <h3 className="panel-title">📊 Live counts</h3>
        <div className="health-grid">
          {tiles.map((t) => (
            <div key={t.label} className="stat-box" title={t.sub ?? t.label}>
              <div className="stat-value">{t.emoji} {t.value}</div>
              <div className="stat-label">{t.label}</div>
            </div>
          ))}
        </div>
      </section>

      {/* ---- recent captures ---- */}
      <section className="panel" aria-label="Recent captures">
        <h3 className="panel-title">🕘 Recent captures</h3>
        {recent.length === 0 ? (
          <p className="text-dim" style={{ fontSize: 12.5 }}>
            Koi capture nahi abhi. Ctrl+C karke popup me "Save" dabao — turant
            yahan dikhega.
          </p>
        ) : (
          <ul className="result-list">
            {recent.map((c: DbOverviewData["recent"][number]) => (
              <li key={c.chatId}>
                <button
                  className="result-item"
                  onClick={() => void openChat(c.chatId)}
                  title="Open this capture"
                >
                  <div className="result-path">
                    <span className="result-kind-badge">
                      {KIND_EMOJI[c.kind] ?? "📥"}
                    </span>
                    {c.kind} · {fmtTime(c.createdAt)}
                  </div>
                  <div className="result-title">{c.title || "(untitled)"}</div>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
