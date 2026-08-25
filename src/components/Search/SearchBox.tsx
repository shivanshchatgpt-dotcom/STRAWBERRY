import { useEffect, useRef, useState } from "react";
import { useAppStore } from "../../store/appStore";

export function SearchBox() {
  const searchQuery = useAppStore((s) => s.searchQuery);
  const setSearchQuery = useAppStore((s) => s.setSearchQuery);
  const runSearch = useAppStore((s) => s.runSearch);
  const clearSearch = useAppStore((s) => s.clearSearch);
  const [local, setLocal] = useState(searchQuery);
  const timer = useRef<number | undefined>(undefined);

  // Debounced live search.
  useEffect(() => {
    window.clearTimeout(timer.current);
    if (local.trim() === "") {
      if (searchQuery !== "") clearSearch();
      return;
    }
    timer.current = window.setTimeout(() => {
      if (useAppStore.getState().searchQuery !== local) {
        setSearchQuery(local);
        void useAppStore.getState().runSearch();
      }
    }, 300);
    return () => window.clearTimeout(timer.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [local]);

  // Keep local in sync when cleared via Esc/global handler.
  useEffect(() => {
    if (searchQuery === "" && local !== "" && document.activeElement?.id !== "global-search") {
      setLocal("");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchQuery]);

  return (
    <div className="search-wrap">
      <input
        id="global-search"
        type="text"
        placeholder="Search chats… (Ctrl/Cmd+K)"
        value={local}
        onChange={(e) => setLocal(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            setSearchQuery(local);
            void runSearch();
          }
          if (e.key === "Escape") {
            (e.target as HTMLInputElement).blur();
            setLocal("");
            clearSearch();
          }
        }}
        aria-label="Search chats"
      />
      <span className="search-hint">⏎</span>
    </div>
  );
}

interface ScopePickerProps {
  value: "global" | "root" | "folder";
  onChange: (kind: "global" | "root" | "folder", id: string | null, label: string) => void;
}

/** Scope selector shown on the search results view. */
export function ScopePicker({ value, onChange }: ScopePickerProps) {
  const roots = useAppStore((s) => s.roots);
  const currentRootId = useAppStore((s) => s.currentRootId);
  const currentNodeId = useAppStore((s) => s.currentNodeId);
  const breadcrumb = useAppStore((s) => s.breadcrumb);

  const rootName =
    roots.find((r) => r.id === currentRootId)?.name ?? "This index";
  const folderName =
    [...breadcrumb].reverse().find((b) => b.kind === "folder")?.label ??
    "This folder";

  const canScopeFolder = currentNodeId != null || breadcrumb.some((b) => b.kind === "folder");

  return (
    <select
      id="scope-select"
      value={value}
      onChange={(e) => {
        const kind = e.target.value as typeof value;
        if (kind === "global") onChange("global", null, "Global");
        else if (kind === "root") onChange("root", currentRootId, rootName);
        else onChange("folder", currentNodeId, folderName);
      }}
    >
      <option value="global">Global</option>
      {currentRootId && <option value="root">{rootName}</option>}
      {canScopeFolder && <option value="folder">{folderName}</option>}
    </select>
  );
}
