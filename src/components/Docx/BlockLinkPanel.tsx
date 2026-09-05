import { useEffect, useState } from "react";
import { api, type DocBlockLink, type SearchPage } from "../../lib/api";

const STORAGE_KEY = "__strawberry_new_memory_from_block";

export interface NewMemoryFromBlock {
  blockId: string;
  documentId: string;
  blockType: string;
  title: string;
  content: string;
}

export function storeNewMemoryFromBlock(data: NewMemoryFromBlock) {
  try {
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch {}
}

export function consumeNewMemoryFromBlock(): NewMemoryFromBlock | null {
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    sessionStorage.removeItem(STORAGE_KEY);
    return JSON.parse(raw) as NewMemoryFromBlock;
  } catch {
    return null;
  }
}

/**
 * 🔗 BlockLinkPanel — shows memories linked to a DOCX block, with
 * controls to add/remove links via the backend.
 */
export function BlockLinkPanel({
  blockId,
  documentId,
  blockType,
  blockContent,
  blockTitle,
  onNavigateToMemory,
}: {
  blockId: string;
  documentId: string;
  blockType: string;
  blockContent: string;
  blockTitle: string;
  onNavigateToMemory: (memoryId: string) => void;
}) {
  const [links, setLinks] = useState<DocBlockLink[]>([]);
  const [memoryCache, setMemoryCache] = useState<Record<string, { title: string; kind: string } | null>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchPage, setSearchPage] = useState<SearchPage | null>(null);
  const [searching, setSearching] = useState(false);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const list = await api.docxListBlockMemories(blockId);
      setLinks(list);
      // Pre-fetch memory titles.
      const cache: Record<string, { title: string; kind: string } | null> = {};
      for (const l of list) {
        if (l.memoryId in memoryCache) {
          cache[l.memoryId] = memoryCache[l.memoryId];
          continue;
        }
        try {
          const m = await api.memoryGet(l.memoryId);
          cache[l.memoryId] = m ? { title: m.title, kind: m.kind } : null;
        } catch {
          cache[l.memoryId] = null;
        }
      }
      setMemoryCache((prev) => ({ ...prev, ...cache }));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [blockId]);

  // Debounced search.
  useEffect(() => {
    if (!showAdd) return;
    const h = setTimeout(async () => {
      setSearching(true);
      try {
        const r = await api.memorySearch({
          text: searchQuery,
          limit: 8,
        });
        setSearchPage(r);
      } catch (e) {
        setError(String(e));
      } finally {
        setSearching(false);
      }
    }, 200);
    return () => clearTimeout(h);
  }, [searchQuery, showAdd]);

  async function handleLink(memoryId: string) {
    setError(null);
    try {
      await api.docxLinkBlockToMemory({
        blockId,
        documentId,
        blockType,
        memoryId,
      });
      setShowAdd(false);
      setSearchQuery("");
      setSearchPage(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleUnlink(memoryId: string) {
    setError(null);
    try {
      await api.docxUnlinkBlockMemory(blockId, memoryId);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="memory-block-link-panel">
      {error && <div className="memory-error-banner">{error}</div>}

      <div className="memory-block-link-row">
        <span className="memory-block-link-label">
          🔗 Linked memories ({links.length})
        </span>
        <button
          className="memory-rel-add-btn"
          onClick={() => setShowAdd((v) => !v)}
        >
          {showAdd ? "Cancel" : "+ Link Memory"}
        </button>
        <button
          className="memory-rel-add-btn"
          onClick={() => {
            storeNewMemoryFromBlock({
              blockId,
              documentId,
              blockType,
              title: blockTitle || "Memory from block",
              content: blockContent,
            });
            onNavigateToMemory("__new_memory__");
          }}
        >
          + New Memory
        </button>
      </div>

      {loading && <span className="memory-muted">Loading…</span>}

      {links.length > 0 && (
        <ul className="memory-block-link-list">
          {links.map((l) => {
            const meta = memoryCache[l.memoryId];
            return (
              <li key={l.id} className="memory-block-link-entry">
                <span className="memory-block-link-id">{l.memoryId}</span>
                {meta && (
                  <span className={`memory-pill memory-pill-${meta.kind}`}>
                    {meta.kind}
                  </span>
                )}
                {meta && (
                  <a
                    className="memory-block-link-title"
                    onClick={(e) => {
                      e.preventDefault();
                      onNavigateToMemory(l.memoryId);
                    }}
                  >
                    {meta.title}
                  </a>
                )}
                <button
                  className="btn btn-small btn-danger"
                  onClick={() => void handleUnlink(l.memoryId)}
                  title="Unlink this memory"
                >
                  Unlink
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {links.length === 0 && !loading && (
        <span className="memory-muted">
          No memories linked. Click "+ Link Memory" to attach an existing
          memory, or "+ New Memory" to create one from this block.
        </span>
      )}

      {showAdd && (
        <div className="memory-block-link-add">
          <input
            className="memory-search-input"
            type="text"
            placeholder="Search memories by title or content…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            autoFocus
          />
          {searching && <span className="memory-muted">Searching…</span>}
          {searchPage && searchPage.hits.length > 0 && (
            <ul className="memory-search-results">
              {searchPage.hits.map((h) => {
                const alreadyLinked = links.some(
                  (l) => l.memoryId === h.memory.id
                );
                return (
                  <li
                    key={h.memory.id}
                    className="memory-search-item"
                    onClick={() => {
                      if (!alreadyLinked) void handleLink(h.memory.id);
                    }}
                    style={{ opacity: alreadyLinked ? 0.5 : 1 }}
                  >
                    <div className="memory-search-item-head">
                      <span className={`memory-pill memory-pill-${h.memory.kind}`}>
                        {h.memory.kind}
                      </span>
                      <span className="memory-search-item-title">
                        {h.memory.title}
                      </span>
                      {alreadyLinked && (
                        <span className="memory-meta-tag">already linked</span>
                      )}
                    </div>
                    <div className="memory-search-item-content">
                      {h.memory.content.slice(0, 120)}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          {searchPage && searchPage.hits.length === 0 && (
            <span className="memory-muted">No matches.</span>
          )}
        </div>
      )}
    </div>
  );
}
