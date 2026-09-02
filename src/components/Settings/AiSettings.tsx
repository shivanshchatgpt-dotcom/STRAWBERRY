import { useState, useEffect, useCallback } from "react";
import { call } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

type Provider = "ollama" | "byok";

interface AiStatus {
  enabled: boolean;
  activeProvider: string;
  available: boolean;
}

/**
 * 🧠 AI Settings — real provider configuration.
 *
 * Every control connects: React UI → Tauri command → backend config → provider adapter.
 * No fake buttons.
 */
export function AiSettings() {
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [provider, setProvider] = useState<Provider>("ollama");
  const [ollamaModel, setOllamaModel] = useState("llama3");
  const [byokName, setByokName] = useState("");
  const [byokUrl, setByokUrl] = useState("");
  const [byokModel, setByokModel] = useState("");
  const [byokKey, setByokKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const showToast = useAppStore((s) => s.showToast);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await call<AiStatus>("ai_get_status");
      setStatus(s);
      setProvider(s.activeProvider as Provider || "ollama");
    } catch {
      // Status fetch failed — defaults will be shown
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const handleEnableToggle = async (enabled: boolean) => {
    try {
      await call("ai_set_enabled", { enabled });
      await refreshStatus();
      showToast("success", enabled ? "AI enhancement enabled" : "AI enhancement disabled");
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const handleConfigure = async () => {
    try {
      if (provider === "ollama") {
        await call("ai_configure_provider", {
          provider: "ollama",
          model: ollamaModel || "llama3",
        });
      } else {
        if (!byokUrl || !byokModel) {
          showToast("error", "URL and model are required for BYOK");
          return;
        }
        await call("ai_configure_provider", {
          provider: "byok",
          name: byokName || "Custom Provider",
          baseUrl: byokUrl,
          model: byokModel,
          apiKey: byokKey || undefined,
        });
      }
      showToast("success", "Provider configured");
      setByokKey(""); // Clear key from state
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    setTestError(null);
    try {
      const result = await call<string>("ai_test_connection", { provider });
      setTestResult(result);
    } catch (e) {
      setTestError(typeof e === "string" ? e : String(e));
    } finally {
      setTesting(false);
    }
  };

  const handleRemove = async () => {
    try {
      await call("ai_remove_credential", { provider });
      showToast("success", "Provider credentials removed");
      await refreshStatus();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  return (
    <div className="ai-settings">
      <section className="panel pad">
        <div className="section-label">🧠 AI / Intelligence</div>
        <p className="meta-line">
          Strawberry works fully without AI. AI is an optional intelligence amplifier.
        </p>

        {/* Enable/Disable Toggle */}
        <div className="setting-row">
          <label>AI Enhancement</label>
          <button
            className={`btn toggle ${status?.enabled ? "active" : ""}`}
            onClick={() => handleEnableToggle(!status?.enabled)}
          >
            {status?.enabled ? "On" : "Off"}
          </button>
        </div>

        {/* Active Provider */}
        <div className="setting-row">
          <label>Active Mode</label>
          <div className="radio-group">
            <label className="radio">
              <input
                type="radio"
                name="provider"
                value="none"
                checked={status?.activeProvider === "none"}
                onChange={() => handleEnableToggle(false)}
              />
              <span>No AI</span>
            </label>
            <label className="radio">
              <input
                type="radio"
                name="provider"
                value="ollama"
                checked={provider === "ollama"}
                onChange={() => setProvider("ollama")}
              />
              <span>Ollama (Local)</span>
            </label>
            <label className="radio">
              <input
                type="radio"
                name="provider"
                value="byok"
                checked={provider === "byok"}
                onChange={() => setProvider("byok")}
              />
              <span>BYOK (Cloud)</span>
            </label>
          </div>
        </div>

        {/* Provider Status */}
        <div className="status-row">
          <span className={`status-dot ${status?.available ? "green" : "red"}`} />
          <span>{status?.available ? "Connected" : "Not connected"}</span>
        </div>

        {/* Ollama Config */}
        {provider === "ollama" && (
          <div className="provider-config">
            <div className="setting-row">
              <label>Model</label>
              <input
                type="text"
                value={ollamaModel}
                onChange={(e) => setOllamaModel(e.target.value)}
                placeholder="llama3"
                className="input"
              />
            </div>
            <p className="meta-line">
              Requires Ollama running locally. Install:{" "}
              <code>curl -fsSL https://ollama.com/install.sh | sh</code>
            </p>
          </div>
        )}

        {/* BYOK Config */}
        {provider === "byok" && (
          <div className="provider-config">
            <div className="setting-row">
              <label>Provider Name</label>
              <input
                type="text"
                value={byokName}
                onChange={(e) => setByokName(e.target.value)}
                placeholder="OpenAI"
                className="input"
              />
            </div>
            <div className="setting-row">
              <label>Base URL</label>
              <input
                type="text"
                value={byokUrl}
                onChange={(e) => setByokUrl(e.target.value)}
                placeholder="https://api.openai.com/v1"
                className="input"
              />
            </div>
            <div className="setting-row">
              <label>Model</label>
              <input
                type="text"
                value={byokModel}
                onChange={(e) => setByokModel(e.target.value)}
                placeholder="gpt-4o"
                className="input"
              />
            </div>
            <div className="setting-row">
              <label>API Key</label>
              <input
                type="password"
                value={byokKey}
                onChange={(e) => setByokKey(e.target.value)}
                placeholder="sk-..."
                className="input"
              />
            </div>
            <p className="meta-line">
              ⚠️ Cloud providers may receive data. Check your privacy settings.
            </p>
          </div>
        )}

        {/* Actions */}
        <div className="setting-actions">
          <button className="btn primary" onClick={handleConfigure}>
            Save Configuration
          </button>
          <button
            className="btn"
            onClick={handleTest}
            disabled={testing}
          >
            {testing ? <span className="spinner" /> : "🔌"} Test Connection
          </button>
          <button className="btn danger small" onClick={handleRemove}>
            Remove
          </button>
        </div>

        {/* Test Result */}
        {testResult && (
          <div className="test-result ok">✓ {testResult}</div>
        )}
        {testError && (
          <div className="test-result error">✗ {testError}</div>
        )}

        {/* Local vs Cloud Indicator */}
        <div className="trust-indicator">
          <span className="icon">{provider === "ollama" ? "🏠" : "☁️"}</span>
          <span>
            {provider === "ollama"
              ? "Local — data stays on your machine"
              : "Cloud — data may leave your machine"}
          </span>
        </div>
      </section>
    </div>
  );
}
