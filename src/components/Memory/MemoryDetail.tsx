import { useEffect, useState } from "react";
import {
  api,
  type Memory,
  type Relationship,
  type RelationshipType,
  type DocBlockLink,
} from "../../lib/api";
import type { MemoryNav } from "./nav";

/**
 * 📄 Memory Detail — the canonical view of a single memory record.
 *
 * Shows identity, content, source, temporal (with strict separation
 * between first_seen / last_seen / last_viewed / last_copied / last_used),
 * relationships, usage counts, and available actions.
 */
export function MemoryDetail({
  memory,
  onNavigate,
  onChange,
}: {
  memory: Memory;
  onNavigate: MemoryNav;
  onChange: () => void;
}) {
  const [relations, setRelations] = useState<Relationship[]>([]);
  const [relationMemoryCache, setRelationMemoryCache] = useState<Record<string, { title: string; kind: string } | null>>({});
  const [docBlockLinks, setDocBlockLinks] = useState<DocBlockLink[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showNewRel, setShowNewRel] = useState(false);
  const [newRelToId, setNewRelToId] = useState("");
  const [newRelType, setNewRelType] = useState<RelationshipType>("related_to");
  const [newRelEvidence, setNewRelEvidence] = useState("");

  useEffect(() => {
    let cancelled = false;
    api.memoryListRelationships(memory.id)
      .then(async (rels) => {
        if (cancelled) return;
        setRelations(rels);
        // Pre-fetch titles of related memories for display.
        const cache: Record<string, { title: string; kind: string } | null> = {};
        for (const r of rels) {
          const otherId = r.fromId === memory.id ? r.toId : r.fromId;
          if (otherId in relationMemoryCache) {
            cache[otherId] = relationMemoryCache[otherId];
            continue;
          }
          try {
            const other = await api.memoryGet(otherId);
            cache[otherId] = other ? { title: other.title, kind: other.kind } : null;
          } catch {
            cache[otherId] = null;
          }
        }
        if (!cancelled) setRelationMemoryCache((prev) => ({ ...prev, ...cache }));
      })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    // Load DOCX blocks linked to this memory.
    api.docxListMemoryBlocks(memory.id)
      .then((links) => { if (!cancelled) setDocBlockLinks(links); })
      .catch(() => { if (!cancelled) setDocBlockLinks([]); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [memory.id]);

  async function handleDelete() {
    if (!confirm(`Delete memory "${memory.title}"? This cannot be undone.`)) return;
    setBusy("delete");
    setError(null);
    try {
      await api.memoryDelete(memory.id);
      onChange();
      onNavigate.goOverview();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function handleCopy() {
    setBusy("copy");
    setError(null);
    try {
      await navigator.clipboard.writeText(memory.content);
      await api.memoryRecordCopy(memory.id);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function handleUse() {
    setBusy("use");
    setError(null);
    try {
      await api.memoryRecordUse(memory.id);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function handleAddRelationship() {
    if (!newRelToId.trim()) return;
    setBusy("addrel");
    setError(null);
    try {
      await api.memoryCreateRelationship({
        fromId: memory.id,
        toId: newRelToId.trim(),
        relType: newRelType,
        evidence: newRelEvidence.trim() || undefined,
      });
      setShowNewRel(false);
      setNewRelToId("");
      setNewRelEvidence("");
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="memory-detail">
      {error && (
        <div className="memory-error-banner">
          <strong>Error:</strong> {error}
        </div>
      )}

      <div className="memory-detail-header">
        <div>
          <span className={`memory-pill memory-pill-${memory.kind}`}>
            {memory.kind}
          </span>
          <span
            className={`memory-pill memory-pill-privacy-${memory.privacyLevel}`}
            title="Privacy level"
          >
            {memory.privacyLevel}
          </span>
          {memory.redactionState !== "none" && (
            <span className="memory-pill memory-pill-redacted">
              {memory.redactionState}
            </span>
          )}
        </div>
        <h2 className="memory-detail-title">{memory.title}</h2>
        {memory.tags.length > 0 && (
          <div className="memory-detail-tags">
            {memory.tags.map((t) => (
              <span key={t} className="memory-meta-tag">#{t}</span>
            ))}
          </div>
        )}
      </div>

      <div className="memory-detail-actions">
        <button
          className="btn"
          disabled={busy !== null}
          onClick={() => onNavigate.goEdit(memory.id)}
        >
          Edit
        </button>
        <button
          className="btn"
          disabled={busy !== null}
          onClick={handleCopy}
        >
          Copy content
        </button>
        <button
          className="btn"
          disabled={busy !== null}
          onClick={handleUse}
        >
          Mark as used
        </button>
        <button
          className="btn btn-danger"
          disabled={busy !== null}
          onClick={handleDelete}
        >
          Delete
        </button>
      </div>

      <div className="memory-detail-grid">
        <section className="memory-section">
          <h3 className="memory-section-title">Content</h3>
          <pre className="memory-detail-content">{memory.content}</pre>
        </section>

        <section className="memory-section">
          <h3 className="memory-section-title">Source</h3>
          <DefinitionList
            items={[
              ["Source", memory.source],
              ["Source application", memory.sourceApplication],
              ["Source window", memory.sourceWindow],
              ["Source workspace", memory.sourceWorkspace],
              ["Source file", memory.sourceFile],
              ["Source URL", memory.sourceUrl],
              ["Source session", memory.sourceSession],
              ["Project", memory.projectId],
              ["Session", memory.sessionId],
              ["Category", memory.category],
              ["Sensitivity", String(memory.sensitivity)],
              ["Confidence", memory.confidence.toFixed(2)],
              ["Retention (days)", memory.retentionDays != null ? String(memory.retentionDays) : null],
              ["Content hash", memory.contentHash],
            ]}
          />
        </section>

        <section className="memory-section">
          <h3 className="memory-section-title">Timeline</h3>
          <DefinitionList
            items={[
              ["First seen", formatTimestamp(memory.firstSeenAtMs)],
              ["Last seen", formatTimestamp(memory.lastSeenAtMs)],
              ["Last viewed", formatTimestamp(memory.lastViewedAtMs)],
              ["Last copied", formatTimestamp(memory.lastCopiedAtMs)],
              ["Last used", formatTimestamp(memory.lastUsedAtMs)],
              ["Updated at", formatTimestamp(memory.updatedAtMs)],
              ["Created at", formatTimestamp(memory.createdAtMs)],
              ["Occurrence time", formatTimestamp(memory.occurredAtMs)],
            ]}
          />
          <p className="memory-section-note">
            <em>
              Viewed / copied / used are distinct user interactions. Seen
              reflects background observation only. Background processes
              (indexing, FTS, OCR) do not change these.
            </em>
          </p>
        </section>

        <section className="memory-section">
          <h3 className="memory-section-title">Usage</h3>
          <DefinitionList
            items={[
              ["Views", String(memory.viewCount)],
              ["Copies", String(memory.copyCount)],
              ["Uses", String(memory.useCount)],
              [
                "Unused for",
                formatDurationSince(memory.lastUsedAtMs),
              ],
            ]}
          />
        </section>

        <section className="memory-section">
          <h3 className="memory-section-title">
            Relationships
            <button
              className="memory-rel-add-btn"
              onClick={() => setShowNewRel((v) => !v)}
            >
              {showNewRel ? "Cancel" : "+ Add"}
            </button>
          </h3>

          {showNewRel && (
            <div className="memory-rel-form">
              <input
                type="text"
                placeholder="Target memory ID"
                value={newRelToId}
                onChange={(e) => setNewRelToId(e.target.value)}
              />
              <select
                value={newRelType}
                onChange={(e) => setNewRelType(e.target.value as RelationshipType)}
              >
                {(
                  [
                    "related_to", "belongs_to", "created_from", "copied_from",
                    "derived_from", "source_for", "screenshot_of", "captured_during",
                    "attached_to", "references", "part_of", "produced_by",
                    "used_with", "contains", "parent_of", "child_of",
                    "derived_relationship",
                  ] as RelationshipType[]
                ).map((r) => (
                  <option key={r} value={r}>{r}</option>
                ))}
              </select>
              <input
                type="text"
                placeholder="Evidence (optional)"
                value={newRelEvidence}
                onChange={(e) => setNewRelEvidence(e.target.value)}
              />
              <button
                className="btn"
                disabled={busy !== null || !newRelToId.trim()}
                onClick={handleAddRelationship}
              >
                Add
              </button>
            </div>
          )}

          {relations.length === 0 ? (
            <p className="memory-muted">No relationships yet.</p>
          ) : (
            <ul className="memory-rel-list">
              {relations.map((r) => {
                const otherId = r.fromId === memory.id ? r.toId : r.fromId;
                const otherTitle =
                  relationMemoryCache[otherId]?.title ?? otherId;
                const otherKind = relationMemoryCache[otherId]?.kind;
                const dir = r.fromId === memory.id ? "out" : "in";
                return (
                  <li key={r.id} className="memory-rel-item">
                    <div className="memory-rel-row">
                      <span className="memory-rel-dir">{dir === "out" ? "→" : "←"}</span>
                      <span className="memory-rel-type">{r.relType}</span>
                      {otherKind && (
                        <span className={`memory-pill memory-pill-${otherKind}`}>
                          {otherKind}
                        </span>
                      )}
                      <a
                        className="memory-rel-target"
                        onClick={(e) => {
                          e.preventDefault();
                          onNavigate.goDetail(otherId);
                        }}
                      >
                        {otherTitle}
                      </a>
                      <span className="memory-rel-confidence">
                        conf {r.confidence.toFixed(2)}
                      </span>
                    </div>
                    {r.evidence && (
                      <div className="memory-rel-evidence">{r.evidence}</div>
                    )}
                  </li>
                );
              })}
             </ul>
           )}
        </section>

        <section className="memory-section">
          <h3 className="memory-section-title">Linked DOCX blocks</h3>
          {docBlockLinks.length === 0 ? (
            <p className="memory-muted">
              No DOCX blocks linked. Open a document, select a block, and use
              "Link Memory" to connect it.
            </p>
          ) : (
            <ul className="memory-rel-list">
              {docBlockLinks.map((l) => (
                <li key={l.id} className="memory-rel-item">
                  <div className="memory-rel-row">
                    <span className="memory-rel-dir">📄</span>
                    <span className="memory-rel-type">{l.blockType ?? "block"}</span>
                    <span className="memory-rel-target">
                      block {l.blockId} in document {l.documentId}
                    </span>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}

function DefinitionList({ items }: { items: [string, string | null | undefined][] }) {
  const visible = items.filter(([, v]) => v != null && v !== "");
  if (visible.length === 0) {
    return <p className="memory-muted">No data</p>;
  }
  return (
    <dl className="memory-defs">
      {visible.map(([k, v]) => (
        <div key={k} className="memory-def-row">
          <dt className="memory-def-key">{k}</dt>
          <dd className="memory-def-value">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function formatTimestamp(ms: number | null | undefined): string {
  if (!ms) return "Never";
  return new Date(ms).toLocaleString();
}

function formatDurationSince(ms: number | null | undefined): string {
  if (!ms) return "Never used";
  const delta = Date.now() - ms;
  if (delta < 0) return "just now";
  const sec = Math.floor(delta / 1000);
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const days = Math.floor(hr / 24);
  if (days < 30) return `${days}d`;
  return `${Math.floor(days / 30)}mo`;
}
