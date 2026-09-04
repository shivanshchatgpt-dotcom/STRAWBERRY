import { useState } from "react";
import { api } from "../../lib/api";
import type { health } from "../../lib/api";

/**
 * 🩺 Health Lens — read-only disk / cache / home-folder scan.
 * Moved from the dashboard bottom into its own left-nav view.
 */

const fmtGb = (b: number) =>
  b >= 1073741824 ? `${(b / 1073741824).toFixed(1)} GB` : `${Math.round(b / 1048576)} MB`;

export function HealthLensView() {
  const [report, setReport] = useState<health.HealthReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scan = async () => {
    setBusy(true);
    setError(null);
    try {
      setReport(await api.healthReport());
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="content dashboard">
      <header className="dash-head">
        <div>
          <h1 className="dash-title">🩺 Health Lens</h1>
          <div className="meta-line">Read-only scan · nothing is ever deleted</div>
        </div>
        <button className="btn primary" disabled={busy} onClick={() => void scan()}>
          {busy ? <span className="spinner" /> : null} 🩺 Scan Now
        </button>
      </header>

      {error && <div className="dash-error">⚠️ {error}</div>}

      {!report && !busy && (
        <section className="panel">
          <p className="text-dim" style={{ fontSize: 13, lineHeight: 1.6 }}>
            Disk space, cache bloat aur biggest home folders ka snapshot — sab
            read-only. Kuch bhi delete nahi hota. Scan chala ke dekho.
          </p>
        </section>
      )}

      {report && (
        <>
          <section className="stats-grid" aria-label="Disk overview">
            <div className="chart-card">
              <h4 className="chart-title">Disk free</h4>
              <div className="stat-box">
                <div className="stat-value">{fmtGb(report.diskFreeBytes)}</div>
                <div className="stat-label">
                  of {fmtGb(report.diskTotalBytes)} total
                </div>
              </div>
            </div>
            {report.caches.slice(0, 4).map((c) => (
              <div key={c.path} className="chart-card">
                <h4 className="chart-title">Cache</h4>
                <div className="stat-box">
                  <div className="stat-value">{fmtGb(c.bytes)}</div>
                  <div className="stat-label">{c.path}</div>
                </div>
              </div>
            ))}
          </section>

          {report.topHomeDirs.length > 0 && (
            <section className="panel" aria-label="Biggest home folders">
              <h3 className="panel-title">🏠 Biggest home folders</h3>
              <ul className="text-dim" style={{ fontSize: 12.5, lineHeight: 1.8 }}>
                {report.topHomeDirs.map((d) => (
                  <li key={d.path}>
                    <b>{fmtGb(d.bytes)}</b> — {d.path}
                  </li>
                ))}
              </ul>
            </section>
          )}

          {report.notes.length > 0 && (
            <section className="panel" aria-label="Notes">
              <h3 className="panel-title">📝 Notes</h3>
              <ul className="text-dim" style={{ fontSize: 12.5, lineHeight: 1.8 }}>
                {report.notes.map((n, i) => (
                  <li key={i}>{n}</li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}
    </div>
  );
}
