import { useEffect, useRef, useState } from "react";
import { api, type ImageAsset, type OcrRunResult } from "../../lib/api";

/**
 * 🖼️ Images — real image memory backed by SQLite + filesystem.
 * The user can:
 *   * register an image (path is required; the file must already exist)
 *   * see OCR status from the backend
 *   * trigger OCR (calls `image_ocr_run_next` repeatedly)
 *   * read the OCR text
 *   * delete the image
 */
export function MemoryImages() {
  const [images, setImages] = useState<ImageAsset[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ImageAsset | null>(null);
  const [ocrRunning, setOcrRunning] = useState(false);
  const [newPath, setNewPath] = useState("");
  const [newCaption, setNewCaption] = useState("");
  const [newPrivacyBlocked, setNewPrivacyBlocked] = useState(false);
  const [newSourceApp, setNewSourceApp] = useState("");
  const [ocrLog, setOcrLog] = useState<string[]>([]);
  const [registering, setRegistering] = useState(false);
  const pollRef = useRef<number | null>(null);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const list = await api.imageList(100);
      setImages(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  async function handleRegister(e: React.FormEvent) {
    e.preventDefault();
    if (!newPath.trim()) return;
    setRegistering(true);
    setError(null);
    try {
      await api.imageRegister({
        path: newPath.trim(),
        caption: newCaption.trim() || undefined,
        privacyBlocked: newPrivacyBlocked,
        sourceApp: newSourceApp.trim() || undefined,
      });
      setNewPath("");
      setNewCaption("");
      setNewSourceApp("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setRegistering(false);
    }
  }

  async function handleDelete(img: ImageAsset) {
    if (!confirm(`Delete image "${img.caption ?? img.originalPath}"?`)) return;
    try {
      await api.imageDelete(img.id);
      if (selected?.id === img.id) setSelected(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function runOcrForAll() {
    setOcrRunning(true);
    setError(null);
    setOcrLog([]);
    try {
      let total = 0;
      for (let i = 0; i < 30; i++) {
        const result: OcrRunResult | null = await api.imageOcrRunNext();
        if (!result) break;
        total++;
        setOcrLog((prev) => [
          ...prev,
          `${result.imageId}: ${result.status}${result.engine ? ` (${result.engine})` : ""}${
            result.error ? ` — ${result.error}` : ""
          }`,
        ]);
        if (result.status === "unavailable" || result.status === "failed") break;
      }
      setOcrLog((prev) => [...prev, `Processed ${total} image(s).`]);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setOcrRunning(false);
    }
  }

  // Stop any background polling on unmount.
  useEffect(() => () => {
    if (pollRef.current) window.clearInterval(pollRef.current);
  }, []);

  return (
    <div className="memory-images">
      <form className="memory-image-form" onSubmit={handleRegister}>
        <h3 className="memory-section-title">Register an image</h3>
        {error && <div className="memory-error-banner">{error}</div>}
        <div className="memory-form-grid">
          <div className="memory-form-field memory-form-field-wide">
            <label>Absolute path *</label>
            <input
              type="text"
              value={newPath}
              onChange={(e) => setNewPath(e.target.value)}
              placeholder="/home/user/Pictures/screenshot.png"
              required
            />
          </div>
          <div className="memory-form-field">
            <label>Caption</label>
            <input
              type="text"
              value={newCaption}
              onChange={(e) => setNewCaption(e.target.value)}
            />
          </div>
          <div className="memory-form-field">
            <label>Source app</label>
            <input
              type="text"
              value={newSourceApp}
              onChange={(e) => setNewSourceApp(e.target.value)}
            />
          </div>
          <div className="memory-form-field memory-form-field-checkbox">
            <label>
              <input
                type="checkbox"
                checked={newPrivacyBlocked}
                onChange={(e) => setNewPrivacyBlocked(e.target.checked)}
              />
              Privacy-sensitive (skips OCR)
            </label>
          </div>
        </div>
        <div className="memory-form-actions">
          <button className="btn" type="submit" disabled={registering}>
            {registering ? "Registering…" : "Register image"}
          </button>
          <button
            className="btn"
            type="button"
            onClick={() => void runOcrForAll()}
            disabled={ocrRunning || images.length === 0}
          >
            {ocrRunning ? "Running OCR…" : "Run OCR on pending"}
          </button>
        </div>
      </form>

      {ocrLog.length > 0 && (
        <div className="memory-ocr-log">
          <h4>OCR run log</h4>
          <ol>
            {ocrLog.map((line, i) => (
              <li key={i}>{line}</li>
            ))}
          </ol>
        </div>
      )}

      <h3 className="memory-section-title">{images.length} image(s)</h3>

      {loading && <p>Loading…</p>}
      {!loading && images.length === 0 && (
        <div className="memory-empty">
          <h3 className="memory-empty-title">No images yet</h3>
          <p className="memory-empty-body">
            Register an image above to add it to the OCR queue.
            The backend will run local OCR (Tesseract on Linux/macOS)
            when you click "Run OCR on pending".
          </p>
        </div>
      )}

      <div className="memory-image-grid">
        {images.map((img) => (
          <div
            key={img.id}
            className={`memory-image-card${selected?.id === img.id ? " selected" : ""}`}
            onClick={() => setSelected(img)}
          >
            <div className="memory-image-card-thumb">🖼️</div>
            <div className="memory-image-card-meta">
              <div className="memory-image-card-name">
                {img.caption ?? img.originalPath.split("/").pop()}
              </div>
              <div className="memory-image-card-path">
                {img.originalPath}
              </div>
              <div className="memory-image-card-badges">
                <span className={`memory-pill memory-pill-ocr-${img.ocrStatus}`}>
                  ocr: {img.ocrStatus}
                </span>
                <span className={`memory-pill memory-pill-thumbnail-${img.thumbnailStatus}`}>
                  thumb: {img.thumbnailStatus}
                </span>
                {img.privacyBlocked && (
                  <span className="memory-pill memory-pill-privacy-private">
                    privacy
                  </span>
                )}
              </div>
            </div>
            <div className="memory-image-card-actions">
              <button
                className="btn btn-small btn-danger"
                onClick={(e) => {
                  e.stopPropagation();
                  void handleDelete(img);
                }}
              >
                Delete
              </button>
            </div>
          </div>
        ))}
      </div>

      {selected && (
        <div className="memory-image-detail">
          <h3 className="memory-section-title">
            {selected.caption ?? selected.originalPath.split("/").pop()}
          </h3>
          <dl className="memory-defs">
            <div className="memory-def-row">
              <dt>Path</dt>
              <dd>{selected.originalPath}</dd>
            </div>
            <div className="memory-def-row">
              <dt>MIME</dt>
              <dd>{selected.mimeType ?? "—"}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Source app</dt>
              <dd>{selected.sourceApp ?? "—"}</dd>
            </div>
            <div className="memory-def-row">
              <dt>Source window</dt>
              <dd>{selected.sourceWindow ?? "—"}</dd>
            </div>
            <div className="memory-def-row">
              <dt>OCR status</dt>
              <dd>{selected.ocrStatus}</dd>
            </div>
            <div className="memory-def-row">
              <dt>OCR text</dt>
              <dd>
                <pre className="memory-ocr-text">{selected.ocrText ?? "(none yet)"}</pre>
              </dd>
            </div>
          </dl>
          <p className="memory-section-note">
            Raw OCR text may be partially redacted by the backend before
            it is stored in search indices.
          </p>
        </div>
      )}
    </div>
  );
}
