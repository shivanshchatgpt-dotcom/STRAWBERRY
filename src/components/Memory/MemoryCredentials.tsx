import { useEffect, useState } from "react";
import { api, type CredentialMetadata, type SecretStoreStatus } from "../../lib/api";

/**
 * 🔐 Credentials — secure UI for credential metadata.
 *
 * SECURITY INVARIANTS:
 *   * The list and search endpoints NEVER return the secret.
 *   * The user must explicitly click "Reveal" to see a secret.
 *   * Revealed secrets live in component state only — never logged,
 *     never sent back to the backend, and never sent over the network.
 *   * The raw secret is wiped from local state when the user navigates
 *     away or clicks "Hide".
 */
export function MemoryCredentials() {
  const [items, setItems] = useState<CredentialMetadata[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [storeStatus, setStoreStatus] = useState<SecretStoreStatus | null>(null);

  // Form state
  const [formTitle, setFormTitle] = useState("");
  const [formService, setFormService] = useState("");
  const [formAccount, setFormAccount] = useState("");
  const [formUsername, setFormUsername] = useState("");
  const [formEnv, setFormEnv] = useState("");
  const [formHost, setFormHost] = useState("");
  const [formProject, setFormProject] = useState("");
  const [formUrl, setFormUrl] = useState("");
  const [formNotes, setFormNotes] = useState("");
  const [formSecret, setFormSecret] = useState("");

  // Reveal state — strictly local
  const [revealFor, setRevealFor] = useState<string | null>(null);
  const [revealedSecret, setRevealedSecret] = useState<string | null>(null);

  useEffect(() => {
    void api.credentialSecretStoreStatus()
      .then(setStoreStatus)
      .catch(() => setStoreStatus({ available: false, backend: "unknown" }));
  }, []);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      if (query.trim()) {
        const results = await api.credentialSearch(query.trim(), 50);
        setItems(results);
      } else {
        // No query — list is intentionally not exposed (no public list_creds
        // command) to avoid leaking metadata; the search command is the
        // only way to discover credentials.
        setItems([]);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); /* eslint-disable-next-line */ }, [query]);

  async function handleReveal(c: CredentialMetadata) {
    try {
      const bytes = await api.credentialReveal(c.id);
      if (bytes && bytes.length > 0) {
        const text = new TextDecoder("utf-8").decode(new Uint8Array(bytes));
        setRevealFor(c.id);
        setRevealedSecret(text);
      } else {
        setError("No secret stored for this credential");
      }
    } catch (e) {
      setError(String(e));
    }
  }

  function handleHide() {
    setRevealFor(null);
    setRevealedSecret(null);
  }

  async function handleCopy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      setError(`Clipboard copy failed: ${String(e)}`);
    }
  }

  async function handleDelete(c: CredentialMetadata) {
    if (!confirm(`Delete credential "${c.service}${c.account ? ` (${c.account})` : ""}"?`)) return;
    try {
      await api.credentialDelete(c.id);
      if (revealFor === c.id) handleHide();
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!formTitle.trim() || !formService.trim()) {
      setError("Title and service are required.");
      return;
    }
    // Refuse to save secrets if the keyring is unavailable.
    if (formSecret && storeStatus && !storeStatus.available) {
      setError("OS keyring is unavailable — cannot store secrets. Disable the secret field or configure the keyring.");
      return;
    }
    setError(null);
    try {
      // The backend stores the secret in the OS keyring via the
      // secret_store module. We send the raw secret bytes; the backend
      // encodes them and persists via the secure backend.
      const enc = new TextEncoder().encode(formSecret);
      const cipher = Array.from(enc);
      const nonce = Array.from(new TextEncoder().encode("nonce-v1"));
      await api.credentialCreate({
        title: formTitle.trim(),
        service: formService.trim(),
        account: formAccount || undefined,
        username: formUsername || undefined,
        environment: formEnv || undefined,
        host: formHost || undefined,
        project: formProject || undefined,
        url: formUrl || undefined,
        notes: formNotes || undefined,
        secretCiphertext: formSecret ? cipher : undefined,
        secretNonce: formSecret ? nonce : undefined,
      });
      // Wipe local form state
      setFormTitle(""); setFormService(""); setFormAccount(""); setFormUsername("");
      setFormEnv(""); setFormHost(""); setFormProject(""); setFormUrl(""); setFormNotes("");
      setFormSecret("");
      setShowCreate(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="memory-credentials">
      {error && <div className="memory-error-banner">{error}</div>}

      {storeStatus && !storeStatus.available && (
        <div className="memory-cred-warning">
          🔐 OS keyring unavailable on this platform (backend: {storeStatus.backend}).
          Secrets cannot be stored. The form below will refuse to save.
        </div>
      )}
      {storeStatus && storeStatus.available && (
        <div className="memory-cred-info">
          🔐 Secrets are stored in the OS keyring ({storeStatus.backend}).
          The database never contains raw secret bytes.
        </div>
      )}

      <div className="memory-cred-warning">
        Secret values are never returned by search. Use "Reveal" below
        to view a specific secret — it stays in this tab only and is wiped
        when you navigate away.
      </div>

      <div className="memory-cred-toolbar">
        <input
          className="memory-search-input"
          type="text"
          placeholder="Search credentials by service, account, host, project, notes…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className="btn"
          onClick={() => setShowCreate((v) => !v)}
        >
          {showCreate ? "Cancel" : "+ New credential"}
        </button>
      </div>

      {showCreate && (
        <form className="memory-form" onSubmit={handleCreate}>
          <h3 className="memory-section-title">New credential</h3>
          <div className="memory-form-grid">
            <div className="memory-form-field">
              <label>Title *</label>
              <input
                type="text"
                value={formTitle}
                onChange={(e) => setFormTitle(e.target.value)}
                required
              />
            </div>
            <div className="memory-form-field">
              <label>Service *</label>
              <input
                type="text"
                value={formService}
                onChange={(e) => setFormService(e.target.value)}
                required
                placeholder="e.g. ExampleService"
              />
            </div>
            <div className="memory-form-field">
              <label>Account</label>
              <input
                type="text"
                value={formAccount}
                onChange={(e) => setFormAccount(e.target.value)}
              />
            </div>
            <div className="memory-form-field">
              <label>Username</label>
              <input
                type="text"
                value={formUsername}
                onChange={(e) => setFormUsername(e.target.value)}
              />
            </div>
            <div className="memory-form-field">
              <label>Environment</label>
              <input
                type="text"
                value={formEnv}
                onChange={(e) => setFormEnv(e.target.value)}
                placeholder="production / staging / dev"
              />
            </div>
            <div className="memory-form-field">
              <label>Host / device</label>
              <input
                type="text"
                value={formHost}
                onChange={(e) => setFormHost(e.target.value)}
              />
            </div>
            <div className="memory-form-field">
              <label>Project</label>
              <input
                type="text"
                value={formProject}
                onChange={(e) => setFormProject(e.target.value)}
              />
            </div>
            <div className="memory-form-field">
              <label>URL</label>
              <input
                type="text"
                value={formUrl}
                onChange={(e) => setFormUrl(e.target.value)}
              />
            </div>
            <div className="memory-form-field memory-form-field-wide">
              <label>Notes (non-secret)</label>
              <textarea
                value={formNotes}
                onChange={(e) => setFormNotes(e.target.value)}
                rows={2}
              />
            </div>
            <div className="memory-form-field memory-form-field-wide">
              <label>Secret (will be stored encrypted at rest when a keychain is configured)</label>
              <input
                type="password"
                value={formSecret}
                onChange={(e) => setFormSecret(e.target.value)}
                autoComplete="off"
              />
            </div>
          </div>
          <div className="memory-form-actions">
            <button type="submit" className="btn">Save credential</button>
          </div>
        </form>
      )}

      <h3 className="memory-section-title">
        {items.length} credential{items.length === 1 ? "" : "s"}
        {query.trim() ? ` matching "${query}"` : " — search to see results"}
      </h3>

      {loading && <p>Loading…</p>}
      {!loading && items.length === 0 && (
        <div className="memory-empty">
          <h3 className="memory-empty-title">No credentials</h3>
          <p className="memory-empty-body">
            {query.trim()
              ? "No matches for your query."
              : "Search above by service, account, host, project, or notes to find credentials. Secret bodies are never shown in normal search."}
          </p>
        </div>
      )}

      <ul className="memory-cred-list">
        {items.map((c) => (
          <li key={c.id} className="memory-cred-item">
            <div className="memory-cred-row">
              <span className="memory-cred-service">{c.service}</span>
              {c.account && <span className="memory-cred-account">{c.account}</span>}
              {c.username && <span className="memory-cred-username">@{c.username}</span>}
              {c.environment && (
                <span className="memory-pill memory-pill-privacy-normal">
                  {c.environment}
                </span>
              )}
              {c.project && <span className="memory-meta-tag">📁 {c.project}</span>}
              {c.host && <span className="memory-meta-tag">🖥 {c.host}</span>}
              {c.url && <span className="memory-meta-tag">🔗 {c.url}</span>}
              {c.secretSet && (
                <span className="memory-pill memory-pill-privacy-secret">
                  secret set
                </span>
              )}
              {c.lastUsedAtMs && (
                <span className="memory-meta-tag">
                  used {formatRelative(c.lastUsedAtMs)}
                </span>
              )}
            </div>
            {c.notes && <div className="memory-cred-notes">{c.notes}</div>}
            <div className="memory-cred-actions">
              {c.secretSet && (
                revealFor === c.id ? (
                  <>
                    <button
                      className="btn btn-small"
                      onClick={() => handleHide()}
                    >
                      Hide
                    </button>
                    <button
                      className="btn btn-small"
                      onClick={() => revealedSecret && handleCopy(revealedSecret)}
                    >
                      Copy
                    </button>
                    <pre className="memory-cred-secret">{revealedSecret}</pre>
                  </>
                ) : (
                  <button
                    className="btn btn-small"
                    onClick={() => void handleReveal(c)}
                  >
                    Reveal secret
                  </button>
                )
              )}
              <button
                className="btn btn-small btn-danger"
                onClick={() => void handleDelete(c)}
              >
                Delete
              </button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

function formatRelative(ms: number): string {
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
