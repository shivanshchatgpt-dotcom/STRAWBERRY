import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { inbox } from "../../lib/api";
import { useAppStore } from "../../store/appStore";
import { formatDate } from "../../lib/utils";

type KindFilter = "all" | "note" | "code" | "error" | "url";

const KIND_META: Record<KindFilter, { label: string; icon: string }> = {
  all: { label: "All", icon: "📥" },
  note: { label: "Notes", icon: "📝" },
  code: { label: "Code", icon: "🧩" },
  error: { label: "Errors", icon: "🔥" },
  url: { label: "Links", icon: "🔗" },
};

/**
 * 📥 Universal Inbox — every clipboard capture, one place.
 * "Maine 5 jagah same cheez likhi hai" → yahan sab kuch pehle se hai.
 */
export function InboxView() {
  const openChat = useAppStore((s) => s.openChat);
  const [kind, setKind] = useState<KindFilter>("all");
  const [items, setItems] = useState<inbox.InboxItem[] | null>(null);
  const [counts, setCounts] = useState<inbox.InboxCounts | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (k: KindFilter) => {
    try {
      const [list, c] = await Promise.all([
        api.getInboxItems(k === "all" ? null : k, 200),
        api.getInboxCounts(),
      ]);
      setItems(list);
      setCounts(c);
      setError(null);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setItems([]);
    }
  }, []);

  useEffect(() => {
    void refresh(kind);
  }, [kind, refresh]);

  const remove = async (chatId: string) => {
    try {
      await api.deleteInboxItem(chatId);
      await refresh(kind);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  return (
    <div className="content inbox-view">
      <header className="page-head">
        <div>
          <h1>📥 Universal Inbox</h1>
          <div className="sub">
            Har clipboard capture — notes, code, errors, links. Auto-classified, searchable.
          </div>
        </div>
        <button className="btn" onClick={() => void refresh(kind)}>
          ↻ Refresh
        </button>
      </header>

      {error && <div className="dash-error">⚠️ {error}</div>}

      {/* Kind filter chips */}
      <div className="inbox-filters">
        {(Object.keys(KIND_META) as KindFilter[]).map((k) => {
          const count = counts ? counts[k] : null;
          return (
            <button
              key={k}
              className={`inbox-filter${kind === k ? " active" : ""}`}
              onClick={() => setKind(k)}
            >
              <span aria-hidden>{KIND_META[k].icon}</span> {KIND_META[k].label}
              {count !== null && <span className="inbox-count">{count}</span>}
            </button>
          );
        })}
      </div>

      {/* Items list */}
      {items === null ? (
        <div className="loading-block">
          <span className="spinner" /> Loading inbox…
        </div>
      ) : items.length === 0 ? (
        <div className="empty-state">
          <div className="empty-icon" aria-hidden>📭</div>
          <h3>Inbox khaali hai</h3>
          <p>
            Kuch bhi copy karo — capture daemon usse yahan save kar dega
            (type ke hisaab se classify karke). Double-copy "sb!" se handoff packet banta hai.
          </p>
        </div>
      ) : (
        <div className="inbox-list">
          {items.map((it) => (
            <div key={it.chatId} className={`inbox-item kind-${it.kind ?? "note"}`}>
              <span className="inbox-kind" aria-hidden>
                {KIND_META[(it.kind as KindFilter) ?? "note"]?.icon ?? "📝"}
              </span>
              <div
                className="inbox-main"
                role="button"
                tabIndex={0}
                onClick={() => void openChat(it.chatId)}
                onKeyDown={(e) => e.key === "Enter" && void openChat(it.chatId)}
              >
                <div className="inbox-title">{it.title}</div>
                {it.preview && <div className="inbox-preview">{it.preview}</div>}
              </div>
              <time className="inbox-ts">{formatDate(it.createdAt)}</time>
              <button
                className="icon-btn danger"
                title="Delete capture"
                onClick={() => void remove(it.chatId)}
              >
                🗑
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
