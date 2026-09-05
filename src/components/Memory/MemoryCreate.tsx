import { useEffect, useState } from "react";
import {
  api,
  type Memory,
  type MemoryKind,
  type PrivacyLevel,
  type RedactionState,
} from "../../lib/api";
import type { MemoryNav } from "./nav";
import { consumeNewMemoryFromBlock } from "../Docx/BlockLinkPanel";

interface PrefillData {
  title: string;
  content: string;
  blockId: string;
  documentId: string;
  blockType: string;
}

/**
 * ✏️ Memory Create / Edit — real form bound to `memory_create` and
 * `memory_update`. The update path preserves the stable memory id and
 * all relationships, timestamps, DOCX links, and assets.
 */
export function MemoryCreate({
  memory,
  onNavigate,
}: {
  memory?: Memory;
  onNavigate: MemoryNav;
}) {
  const isEdit = !!memory;
  const [prefill, setPrefill] = useState<PrefillData | null>(null);
  const [kind, setKind] = useState<MemoryKind>(
    (memory?.kind as MemoryKind) ?? "generic"
  );
  const [title, setTitle] = useState(memory?.title ?? "");
  const [content, setContent] = useState(memory?.content ?? "");
  const [tagsText, setTagsText] = useState(memory?.tags.join(" ") ?? "");
  const [project, setProject] = useState(memory?.projectId ?? "");
  const [category, setCategory] = useState(memory?.category ?? "");
  const [sourceApp, setSourceApp] = useState(memory?.sourceApplication ?? "");
  const [sourceFile, setSourceFile] = useState(memory?.sourceFile ?? "");
  const [sourceUrl, setSourceUrl] = useState(memory?.sourceUrl ?? "");
  const [sourceSession, setSourceSession] = useState(memory?.sourceSession ?? "");
  const [privacyLevel, setPrivacyLevel] = useState<PrivacyLevel>(
    (memory?.privacyLevel as PrivacyLevel) ?? "normal"
  );
  const [redactionState, setRedactionState] = useState<RedactionState>(
    (memory?.redactionState as RedactionState) ?? "none"
  );
  const [sensitivity, setSensitivity] = useState<number>(memory?.sensitivity ?? 1);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isEdit) return;
    const data = consumeNewMemoryFromBlock();
    if (data) {
      setPrefill(data);
      setTitle(data.title);
      setContent(data.content);
    }
  }, [isEdit]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim()) {
      setError("Title is required.");
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const tags = tagsText
        .split(/[\s,]+/)
        .map((t) => t.trim())
        .filter((t) => t.length > 0);
      if (isEdit && memory) {
        await api.memoryUpdate({
          id: memory.id,
          title: title.trim(),
          content,
          kind,
          tags,
          category: category || null,
          project: project || null,
          session: null,
          sourceApplication: sourceApp || null,
          sourceWindow: null,
          sourceWorkspace: null,
          sourceFile: sourceFile || null,
          sourceUrl: sourceUrl || null,
          sourceSession: sourceSession || null,
          privacyLevel,
          sensitivity,
          redactionState,
        });
      } else {
        const newMemoryId = await api.memoryCreate({
          kind,
          title: title.trim(),
          content,
          source: "user",
          project: project || undefined,
          tags,
          sourceApplication: sourceApp || undefined,
          sourceUrl: sourceUrl || undefined,
          sourceFile: sourceFile || undefined,
        });
        if (
          privacyLevel !== "normal" ||
          redactionState !== "none" ||
          sensitivity !== 1
        ) {
          await api.memoryUpdate({
            id: newMemoryId,
            privacyLevel,
            sensitivity,
            redactionState,
          });
        }
        if (prefill) {
          try {
            await api.docxLinkBlockToMemory({
              blockId: prefill.blockId,
              documentId: prefill.documentId,
              blockType: prefill.blockType,
              memoryId: newMemoryId,
            });
          } catch {
            // Linking is best-effort; don't fail the create if it doesn't work
          }
        }
      }
      onNavigate.goOverview();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form className="memory-form" onSubmit={handleSubmit}>
      <h2 className="memory-section-title">
        {isEdit ? `Edit: ${memory?.title}` : prefill ? "New memory from block" : "New memory"}
      </h2>

      {error && <div className="memory-error-banner">{error}</div>}

      <div className="memory-form-grid">
        <div className="memory-form-field">
          <label>Kind</label>
          <select value={kind} onChange={(e) => setKind(e.target.value as MemoryKind)}>
            <option value="working">working</option>
            <option value="episodic">episodic</option>
            <option value="semantic">semantic</option>
            <option value="project">project</option>
            <option value="procedural">procedural</option>
            <option value="document">document</option>
            <option value="image">image</option>
            <option value="block">block</option>
            <option value="credential">credential</option>
            <option value="generic">generic</option>
          </select>
        </div>

        <div className="memory-form-field">
          <label>Privacy level</label>
          <select
            value={privacyLevel}
            onChange={(e) => setPrivacyLevel(e.target.value as PrivacyLevel)}
          >
            <option value="public">public</option>
            <option value="normal">normal</option>
            <option value="sensitive">sensitive</option>
            <option value="private">private</option>
            <option value="secret">secret</option>
          </select>
        </div>

        <div className="memory-form-field memory-form-field-wide">
          <label>Title *</label>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            required
            autoFocus
          />
        </div>

        <div className="memory-form-field memory-form-field-wide">
          <label>Content</label>
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            rows={8}
          />
        </div>

        <div className="memory-form-field">
          <label>Project</label>
          <input
            type="text"
            value={project}
            onChange={(e) => setProject(e.target.value)}
            placeholder="optional"
          />
        </div>

        <div className="memory-form-field">
          <label>Category</label>
          <input
            type="text"
            value={category}
            onChange={(e) => setCategory(e.target.value)}
            placeholder="optional"
          />
        </div>

        <div className="memory-form-field memory-form-field-wide">
          <label>Tags (space- or comma-separated)</label>
          <input
            type="text"
            value={tagsText}
            onChange={(e) => setTagsText(e.target.value)}
            placeholder="e.g. meeting note project-x"
          />
        </div>

        <div className="memory-form-field">
          <label>Source application</label>
          <input
            type="text"
            value={sourceApp}
            onChange={(e) => setSourceApp(e.target.value)}
          />
        </div>

        <div className="memory-form-field">
          <label>Source file</label>
          <input
            type="text"
            value={sourceFile}
            onChange={(e) => setSourceFile(e.target.value)}
          />
        </div>

        <div className="memory-form-field">
          <label>Source URL</label>
          <input
            type="text"
            value={sourceUrl}
            onChange={(e) => setSourceUrl(e.target.value)}
          />
        </div>

        <div className="memory-form-field">
          <label>Source session</label>
          <input
            type="text"
            value={sourceSession}
            onChange={(e) => setSourceSession(e.target.value)}
          />
        </div>

        <div className="memory-form-field">
          <label>Sensitivity (0–10)</label>
          <input
            type="number"
            min={0}
            max={10}
            value={sensitivity}
            onChange={(e) => setSensitivity(Number(e.target.value))}
          />
        </div>

        <div className="memory-form-field">
          <label>Redaction</label>
          <select
            value={redactionState}
            onChange={(e) => setRedactionState(e.target.value as RedactionState)}
          >
            <option value="none">none</option>
            <option value="redacted">redacted</option>
            <option value="blocked">blocked</option>
          </select>
        </div>
      </div>

      <div className="memory-form-actions">
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => onNavigate.goOverview()}
          disabled={submitting}
        >
          Cancel
        </button>
        <button type="submit" className="btn" disabled={submitting}>
          {submitting ? "Saving…" : isEdit ? "Save changes" : "Create memory"}
        </button>
      </div>
    </form>
  );
}
