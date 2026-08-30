import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { alpha } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

function providerUrl(provider: string, modelId: string | null): string | null {
  const m = modelId ?? "";
  switch (provider) {
    case "openrouter":   return m ? `https://openrouter.ai/models/${m}` : null;
    case "huggingface": return m ? (m.includes("/") ? `https://huggingface.co/${m}` : `https://huggingface.co/models?search=${m}`) : null;
    case "github":       return m ? `https://github.com/${m}` : null;
    case "groq":         return m ? `https://console.groq.com/models/${m}` : null;
    case "together":     return m ? `https://api.together.xyz/models/${m}` : null;
    case "cerebras":     return m ? `https://inference.cerebras.ai/models/${m}` : null;
    case "mistral":      return m ? `https://console.mistral.ai/models/${m}` : null;
    default:             return null;
  }
}

function modelIdUrl(source: string, modelId: string | null, baseUrl: string | null): string | null {
  if (!modelId) return null;
  if (source === "openrouter")  return `https://openrouter.ai/models/${modelId}`;
  if (source === "huggingface") return modelId.includes("/") ? `https://huggingface.co/${modelId}` : `https://huggingface.co/models?search=${modelId}`;
  if (source === "github")      return `https://github.com/${modelId}`;
  if (baseUrl)                  return baseUrl;
  return null;
}

function baseUrlLink(baseUrl: string | null): string | null {
  if (!baseUrl) return null;
  const u = baseUrl.trim();
  if (u.includes("openrouter")) return "https://openrouter.ai";
  if (u.includes("groq"))       return "https://console.groq.com";
  if (u.includes("together"))   return "https://api.together.xyz";
  if (u.includes("cerebras"))   return "https://inference.cerebras.ai";
  if (u.includes("mistral"))     return "https://console.mistral.ai";
  if (u.includes("huggingface"))return "https://huggingface.co";
  if (u.includes("deepinfra"))   return "https://deepinfra.com";
  if (u.includes("fireworks"))   return "https://fireworks.ai";
  if (u.includes("cohere"))      return "https://cohere.com";
  return u;
}

function Chip({ label, url }: { label: string; url: string | null }) {
  if (url) {
    return (
      <a href={url} target="_blank" rel="noreferrer" className="alpha-chip">
        {label}
      </a>
    );
  }
  return <span className="alpha-chip">{label}</span>;
}

export function AlphaHunter() {
  const [candidates, setCandidates] = useState<alpha.AlphaCandidate[]>([]);
  const [enabled, setEnabled]       = useState(false);
  const [scanning, setScanning]     = useState(false);
  const [report, setReport]         = useState<string | null>(null);
  const [verifyingId, setVerifyingId] = useState<string | null>(null);
  const [apiKey, setApiKey]         = useState("");
  const [configFor, setConfigFor]   = useState<string | null>(null);
  const [configText, setConfigText]  = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [c, en] = await Promise.all([api.listAlphaCandidates(), api.getAlphaEnabled()]);
      setCandidates(c);
      setEnabled(en);
    } catch { /* silent */ }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  const toggleEnabled = async () => {
    try { await api.setAlphaEnabled(!enabled); setEnabled(!enabled); } catch { /* ignore */ }
  };

  const scan = async () => {
    setScanning(true);
    setReport(null);
    try {
      const r = await api.scanAlpha();
      setReport(r.enabled
        ? `Scanned ${r.scannedSources} sources · checked ${r.itemsChecked} items · ${r.newCandidates} new candidate${r.newCandidates === 1 ? "" : "s"} 🎯`
        : "Alpha Hunter is OFF — enable it above first (network opt-in for privacy).");
      await refresh();
    } catch (e) {
      setReport(typeof e === "string" ? e : String(e));
    } finally {
      setScanning(false);
    }
  };

  const verify = async (id: string) => {
    if (!apiKey.trim() || verifyingId) return;
    setVerifyingId(id);
    try { await api.verifyAlphaCandidate(id, apiKey.trim()); } catch { /* ignore */ }
    await refresh();
    setVerifyingId(null);
  };

  const dismiss = async (id: string) => {
    try { await api.dismissAlphaCandidate(id); await refresh(); } catch { /* ignore */ }
  };

  const showConfig = async (id: string) => {
    if (configFor === id) { setConfigFor(null); setConfigText(null); return; }
    try {
      const text = await api.getAlphaConfig(id);
      setConfigFor(id);
      setConfigText(text);
    } catch { /* ignore */ }
  };

  const copyConfig = async () => {
    if (configText) await useAppStore.getState().copyText(configText, "Alpha config");
  };

  const statusBadge = (s: string) => {
    if (s === "verified") return <span className="alpha-badge alpha-verified">✅ verified</span>;
    if (s === "failed")   return <span className="alpha-badge alpha-failed">❌ failed</span>;
    return                        <span className="alpha-badge alpha-new">🆕 new</span>;
  };

  return (
    <section className="panel" aria-label="Alpha Hunter">
      <h3 className="panel-title">🎯 Alpha Hunter</h3>
      <p className="text-dim" style={{ fontSize: 12.5, margin: "4px 0 10px" }}>
        Scans <b>6 public sources</b> for legit free AI models / API keys:
        HackerNews, Reddit, OpenRouter, GitHub, HuggingFace, Product Hunt.
        No paywalls, no logins, no LLM — pure keyword hunting.
      </p>

      {!enabled && (
        <div className="alpha-report" style={{ marginBottom: 10 }}>
          ⚠️ Alpha Hunter is <b>OFF</b>. Network scanning is opt-in for privacy.
          Enable it below, then hit <b>Scan Sources</b>.
        </div>
      )}

      <div className="quick-row">
        <button className={`btn${enabled ? " primary" : ""}`} onClick={() => void toggleEnabled()}>
          {enabled ? "🟢 Enabled" : "⚪ ENABLE network scanning"}
        </button>
        <button className="btn primary" disabled={scanning || !enabled} onClick={() => void scan()}>
          {scanning ? <span className="spinner" /> : null} 📡 Scan Sources
        </button>
        <input
          className="quick-input" style={{ maxWidth: 280 }}
          type="password" placeholder="API key for live verify (not saved)…"
          value={apiKey} onChange={(e) => setApiKey(e.target.value)}
        />
      </div>

      {report && <div className="alpha-report">{report}</div>}

      <ul className="alpha-list">
        {candidates.map((c) => {
          const pUrl   = c.provider ? providerUrl(c.provider, c.modelId) : null;
          const mUrl   = modelIdUrl(c.source, c.modelId, c.baseUrl);
          const bUrl   = baseUrlLink(c.baseUrl);
          const srcUrl = c.url;

          return (
            <li key={c.id} className="alpha-card">
              <div className="alpha-head">
                {statusBadge(c.status)}
                <span className="alpha-source">{c.source}</span>
                <span className="alpha-score" title="Detector confidence">⚡ {c.score}</span>
                <time className="alpha-time">{c.detectedAt.slice(0, 16).replace("T", " ")}</time>
              </div>

              <div className="alpha-title">
                {srcUrl
                  ? <a href={srcUrl} target="_blank" rel="noreferrer">{c.title}</a>
                  : c.title}
              </div>

              <div className="alpha-chips">
                {c.provider && (
                  <Chip label={`🏷 ${c.provider}`} url={pUrl} />
                )}
                {c.modelId && (
                  <Chip label={`🤖 ${c.modelId}`} url={mUrl} />
                )}
                {c.baseUrl && (
                  <Chip label={`🔗 ${c.baseUrl}`} url={bUrl} />
                )}
              </div>

              {c.notes && <div className="alpha-notes">{c.notes}</div>}

              <div className="alpha-actions">
                <button
                  className="btn"
                  disabled={!apiKey.trim() || verifyingId === c.id || !c.baseUrl || !c.modelId}
                  title={!c.baseUrl || !c.modelId ? "No endpoint/model detected" : !apiKey.trim() ? "Enter API key above" : "Live API test call"}
                  onClick={() => void verify(c.id)}
                >
                  {verifyingId === c.id ? <span className="spinner" /> : null} 🧪 Verify
                </button>
                <button className="btn" onClick={() => void showConfig(c.id)}>⚙️ Config</button>
                <button className="btn" onClick={() => void dismiss(c.id)}>🗑 Dismiss</button>
              </div>

              {configFor === c.id && configText && (
                <div className="alpha-config">
                  <pre className="pre-block brief-text">{configText}</pre>
                  <button className="btn" onClick={() => void copyConfig()}>📋 Copy config</button>
                </div>
              )}
            </li>
          );
        })}
        {candidates.length === 0 && (
          <li className="text-dim">No candidates yet. Hit 📡 Scan Sources to hunt for free-model alphas.</li>
        )}
      </ul>
    </section>
  );
}
