import { useAppStore } from "../../store/appStore";
import type { SearchScopeKind } from "../../lib/types";
import { Breadcrumb } from "../Breadcrumb/Breadcrumb";
import { EmptyState } from "../EmptyState/EmptyState";
import { TreePanel } from "../Tree/TreePanel";
import { formatDate } from "../../lib/utils";
import { ScopePicker } from "./SearchBox";

export function SearchResultsView() {
  const results = useAppStore((s) => s.searchResults);
  const searching = useAppStore((s) => s.searching);
  const query = useAppStore((s) => s.searchQuery.trim());
  const searchScope = useAppStore((s) => s.searchScope);
  const setSearchScope = useAppStore((s) => s.setSearchScope);
  const openChat = useAppStore((s) => s.openChat);

  const scopeKind: SearchScopeKind = searchScope?.kind ?? "global";

  return (
    <>
      <TreePanel />
      <div className="content">
        <Breadcrumb />
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

        <div className="scope-row">
          <label htmlFor="scope-select">Scope:</label>
          <ScopePicker
            value={scopeKind}
            onChange={(kind, id, label) =>
              setSearchScope({ kind, id, label })
            }
          />
        </div>

        {!searching && results && results.length === 0 ? (
          <EmptyState
            icon="🔍"
            title={`No matches for “${query}”`}
            hint="Try a different keyword or widen the scope. Search covers chat titles, first ideas, tags and brief text."
          />
        ) : (
          <div className="result-list">
            {results?.map((r) => (
              <button
                key={r.chatId}
                className="result-item"
                onClick={() => void openChat(r.chatId)}
              >
                <div className="result-path">
                  {[r.rootName, r.folderPath].filter(Boolean).join(" / ")} ·{" "}
                  {formatDate(r.createdAt)}
                </div>
                <div className="result-title">{r.title}</div>
                {r.snippet && <div className="result-snippet">{r.snippet}</div>}
              </button>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
