import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import type { docx } from "../../lib/api";
import { BlockEditor, newBlock } from "./BlockEditor";

/**
 * 📄 DOCX workspace — offline document canvas (COPY → PASTE → ORGANIZE).
 * List on the left, editor center, undo/redo + autosave + search.
 */

const FMT_TIME = (iso: string) => {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso.slice(0, 10);
  }
};

export function DocxView() {
  const [docs, setDocs] = useState<docx.DocxSummary[]>([]);
  const [current, setCurrent] = useState<docx.DocxDocument | null>(null);
  const [selectedBlock, setSelectedBlock] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<docx.DocxSummary[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [exportMsg, setExportMsg] = useState<string | null>(null);

  // undo/redo history (block snapshots, capped).
  const undoStack = useRef<Array<{ title: string; blocks: docx.Block[] }>>([]);
  const redoStack = useRef<Array<{ title: string; blocks: docx.Block[] }>>([]);
  const [histTick, setHistTick] = useState(0);
  const suppressHistory = useRef(false);

  const refreshList = useCallback(async () => {
    try {
      setDocs(await api.docxList());
    } catch {
      /* silent */
    }
  }, []);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  // ── document lifecycle ─────────────────────────────────────────

  const newDoc = async () => {
    setBusy(true);
    try {
      const d = await api.docxNew();
      undoStack.current = [];
      redoStack.current = [];
      setCurrent(d);
      setSelectedBlock(null);
      await refreshList();
    } finally {
      setBusy(false);
    }
  };

  const openDoc = async (id: string) => {
    setBusy(true);
    try {
      const d = await api.docxOpen(id);
      undoStack.current = [];
      redoStack.current = [];
      setCurrent(d);
      setSelectedBlock(null);
      setSearchResults(null);
    } finally {
      setBusy(false);
    }
  };

  const deleteDoc = async (id: string) => {
    try {
      await api.docxDelete(id);
      if (current?.id === id) setCurrent(null);
      await refreshList();
    } catch {
      /* silent */
    }
  };

  // ── autosave (debounced 600ms) ─────────────────────────────────

  const saveTimer = useRef<number | undefined>(undefined);
  const currentRef = useRef<docx.DocxDocument | null>(null);
  currentRef.current = current;

  const onDocChange = useCallback((title: string, blocks: docx.Block[]) => {
    const base = currentRef.current;
    if (!base) return;
    // push undo BEFORE mutation (skip while undoing itself)
    if (!suppressHistory.current) {
      undoStack.current.push({ title: base.title, blocks: base.blocks });
      if (undoStack.current.length > 100) undoStack.current.shift();
      redoStack.current = [];
    }
    setCurrent({ ...base, title, blocks });
    setHistTick((t) => t + 1);

    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      void (async () => {
        const doc = currentRef.current;
        if (!doc) return;
        setSaving(true);
        try {
          await api.docxSave(doc.id, doc.title, doc.blocks);
          await refreshList();
        } catch {
          /* autosave failure is non-fatal; next change retries */
        } finally {
          setSaving(false);
        }
      })();
    }, 600);
  }, [refreshList]);

  // ── undo / redo ────────────────────────────────────────────────

  const undo = useCallback(() => {
    const doc = currentRef.current;
    const prev = undoStack.current.pop();
    if (!doc || !prev) return;
    redoStack.current.push({ title: doc.title, blocks: doc.blocks });
    suppressHistory.current = true;
    setCurrent({ ...doc, ...prev });
    setHistTick((t) => t + 1);
    suppressHistory.current = false;
  }, []);

  const redo = useCallback(() => {
    const doc = currentRef.current;
    const next = redoStack.current.pop();
    if (!doc || !next) return;
    undoStack.current.push({ title: doc.title, blocks: doc.blocks });
    suppressHistory.current = true;
    setCurrent({ ...doc, ...next });
    setHistTick((t) => t + 1);
    suppressHistory.current = false;
  }, []);

  // ── keyboard shortcuts (Ctrl+S save, Ctrl+Z/Y undo/redo) ───────

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!currentRef.current) return;
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      if (e.key === "s") {
        e.preventDefault();
        window.clearTimeout(saveTimer.current);
        const doc = currentRef.current;
        if (doc) {
          setSaving(true);
          void api
            .docxSave(doc.id, doc.title, doc.blocks)
            .then(refreshList)
            .finally(() => setSaving(false));
        }
      } else if (e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      } else if ((e.key === "z" && e.shiftKey) || e.key === "y") {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo, refreshList]);

  // ── search ─────────────────────────────────────────────────────

  const runSearch = useCallback(async () => {
    const q = query.trim();
    if (!q) {
      setSearchResults(null);
      return;
    }
    try {
      setSearchResults(await api.docxSearch(q));
    } catch {
      setSearchResults([]);
    }
  }, [query]);

  // ── export ─────────────────────────────────────────────────────

  const doExport = async (format: "markdown" | "html" | "json") => {
    if (!current) return;
    try {
      const result = await api.docxExport(current.id, format);
      // Download via a local blob URL — offline, no network.
      const blob = new Blob([result.content], { type: "text/plain;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = result.filename;
      a.click();
      URL.revokeObjectURL(url);
      setExportMsg(`Exported ${result.filename}`);
      window.setTimeout(() => setExportMsg(null), 2500);
    } catch (e) {
      setExportMsg(`Export failed: ${String(e)}`);
      window.setTimeout(() => setExportMsg(null), 2500);
    }
  };

  const shown = searchResults ?? docs;

  return (
    <div className="docx-view">
      {/* ── left: document list ── */}
      <aside className="docx-sidebar">
        <div className="docx-sidebar-head">
          <h3>📄 Documents</h3>
          <button className="btn primary" disabled={busy} onClick={() => void newDoc()}>
            + New
          </button>
        </div>
        <div className="docx-search">
          <input
            className="quick-input"
            placeholder="Search docs…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void runSearch()}
          />
          <button className="btn" onClick={() => void runSearch()}>🔍</button>
          {searchResults && (
            <button className="btn" onClick={() => { setSearchResults(null); setQuery(""); }}>✕</button>
          )}
        </div>
        <ul className="docx-list">
          {shown.map((d) => (
            <li key={d.id} className={`docx-list-row${current?.id === d.id ? " active" : ""}`}>
              <button className="docx-list-open" onClick={() => void openDoc(d.id)}>
                <span className="docx-list-title">{d.title || "(untitled)"}</span>
                <span className="docx-list-preview">{d.preview.slice(0, 60)}</span>
                <span className="docx-list-time">{FMT_TIME(d.updatedAt)}</span>
              </button>
              <button className="icon-btn" title="Delete" onClick={() => void deleteDoc(d.id)}>🗑</button>
            </li>
          ))}
          {shown.length === 0 && (
            <li className="text-dim docx-list-empty">
              {searchResults ? "No matches." : "No documents yet."}
            </li>
          )}
        </ul>
      </aside>

      {/* ── center: editor ── */}
      <main className="docx-main">
        {current ? (
          <>
            <header className="docx-toolbar">
              <input
                className="docx-title-input"
                value={current.title}
                onChange={(e) => onDocChange(e.target.value, current.blocks)}
                aria-label="Document title"
              />
              <span className="docx-save-state" data-tick={histTick}>
                {saving ? "saving…" : "saved"}
              </span>
              <button className="btn" disabled={!undoStack.current.length} onClick={undo} title="Undo (Ctrl+Z)">↶</button>
              <button className="btn" disabled={!redoStack.current.length} onClick={redo} title="Redo (Ctrl+Y)">↷</button>
              <button className="btn" onClick={() => onDocChange(current.title, [...current.blocks, newBlock("text")])}>
                + text
              </button>
              <select
                className="quick-select"
                defaultValue=""
                onChange={(e) => {
                  if (e.target.value) {
                    onDocChange(current.title, [...current.blocks, newBlock(e.target.value as docx.BlockType)]);
                    e.target.value = "";
                  }
                }}
                aria-label="Insert block"
              >
                <option value="">+ insert block…</option>
                <option value="heading">Heading</option>
                <option value="table">Table</option>
                <option value="todo">Todo banner</option>
                <option value="tree">Tree</option>
                <option value="chart">Chart</option>
                <option value="formula">Math</option>
                <option value="code">Code</option>
                <option value="callout">Callout</option>
                <option value="divider">Divider</option>
              </select>
              <button className="btn" onClick={() => void doExport("markdown")} title="Export Markdown">⬇ md</button>
              <button className="btn" onClick={() => void doExport("html")} title="Export HTML">⬇ html</button>
              <button className="btn" onClick={() => void doExport("json")} title="Export native backup">⬇ json</button>
            </header>
            {exportMsg && <div className="docx-export-msg">{exportMsg}</div>}
            <BlockEditor
              doc={current}
              onChange={onDocChange}
              selectedId={selectedBlock}
              setSelectedId={setSelectedBlock}
            />
          </>
        ) : (
          <div className="docx-welcome">
            <h2>📄 DOCX Workspace</h2>
            <p className="text-dim">
              Copy-paste karo kahin se bhi — tables, todo lists, code, LaTeX,
              indented trees. Smart Paste unko typed editable blocks me convert
              karta hai. Sab offline, sab local.
            </p>
            <button className="btn primary" onClick={() => void newDoc()}>+ New document</button>
          </div>
        )}
      </main>
    </div>
  );
}
