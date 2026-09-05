import { useState } from "react";
import { useAppStore } from "../../store/appStore";
import { EmptyState } from "../EmptyState/EmptyState";
import { TreePanel } from "../Tree/TreePanel";
import { formatDate } from "../../lib/utils";

/**
 * Unified search results — one list across every entity Strawberry stores:
 * 💬 chats · 📋 todos · 🔥 habits · 📅 events · 👻 insights · 🎯 alpha hits.
 * Chat results open the reader; everything else is shown inline.
 */

const KIND_TABS: { key: string; label: string; emoji: string }[] = [
  { key: "all", label: "All", emoji: "✳️" },
  { key: "chat", label: "Chats", emoji: "💬" },
  { key: "memory", label: "Memories", emoji: "🧠" },
  { key: "todo", label: "Tasks", emoji: "📋" },
  { key: "habit", label: "Habits", emoji: "🔥" },
  { key: "event", label: "Events", emoji: "📅" },
  { key: "insight", label: "Insights", emoji: "👻" },
  { key: "alpha", label: "Alpha", emoji: "🎯" },
];

export function SearchResultsView() {
  const results = useAppStore((s) => s.searchResults);
  const searching = useAppStore((s) => s.searching);
  const hasMore = useAppStore((s) => s.searchMemoryHasMore);
  const loadMoreSearch = useAppStore((s) => s.loadMoreSearch);
  const query = useAppStore((s) => s.searchQuery.trim());
  const openChat = useAppStore((s) => s.openChat);
  const [tab, setTab] = useState<string>("all");

  const openMemory = (memoryId: string) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;
    if (w.__strawberry_open_memory) {
      w.__strawberry_open_memory(memoryId);
    }
  };

  const counts = new Map<string, number>();
  for (const r of results ?? []) counts.set(r.kind, (counts.get(r.kind) ?? 0) + 1);
  const shown = tab === "all" ? results ?? [] : (results ?? []).filter((r) => r.kind === tab);

  return (
    <>
      <TreePanel />
      <div className="content">
        <div className="page-head">
          <div>
            <h1>Search</h1>
            <div className="sub">
              {searching ? (
                <span className="loading-inline">Searching…</span>
              ) : (
                `${results?.length ?? 0} result(s) for “${query}”`
              )}
            </div>
          </div>
        </div>

        <div className="unified-search-tabs">
          {KIND_TABS.map((t) => {
            const n = t.key === "all" ? results?.length ?? 0 : counts.get(t.key) ?? 0;
            return (
              <button
                key={t.key}
                type="button"
                className={`unified-tab${tab === t.key ? " active" : ""}`}
                onClick={() => setTab(t.key)}
              >
                {t.emoji} {t.label}
                {n > 0 && <em> {n}</em>}
              </button>
            );
          })}
        </div>

        {!searching && results && results.length === 0 ? (
          <EmptyState
            icon="🔍"
            title={`No matches for “${query}”`}
            hint="Search covers chats, tasks, habits, calendar events, ghost insights and alpha candidates. Try another keyword."
          />
        ) : (
          <div className="result-list">
            {shown.map((r, i) => (
              <button
                key={`${r.kind}-${r.entityId}-${i}`}
                className="result-item"
                onClick={() => {
                  if (r.kind === "chat") void openChat(r.entityId);
                  else if (r.kind === "memory") openMemory(r.entityId);
                }}
              >
                <div className="result-path">
                  <span className="result-kind-badge">{r.emoji}</span>
                  {r.location || r.kind} · {formatDate(r.createdAt)}
                </div>
                <div className="result-title">{r.title}</div>
                {r.snippet && <div className="result-snippet">{r.snippet}</div>}
              </button>
            ))}
            {shown.length === 0 && results && results.length > 0 && (
              <EmptyState
                icon="🫙"
                title="No results of this kind"
                hint="Switch back to the All tab to see matches from other content types."
              />
            )}
            {(hasMore && (tab === "all" || tab === "memory")) && (
              <div className="memory-search-loadmore" style={{ padding: "16px", textAlign: "center" }}>
                <button
                  className="btn btn-ghost"
                  onClick={() => void loadMoreSearch()}
                  disabled={searching}
                >
                  {searching ? "Loading…" : "Load more memories"}
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </>
  );
}
