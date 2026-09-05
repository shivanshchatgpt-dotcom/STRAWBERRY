import { useEffect, useState } from "react";
import { api, type Memory } from "../../lib/api";
import { useMemoryNav } from "./nav";
import { MemoryOverview } from "./MemoryOverview";
import { MemorySearch } from "./MemorySearch";
import { MemoryDetail } from "./MemoryDetail";
import { MemoryCreate } from "./MemoryCreate";
import { MemoryImages } from "./MemoryImages";
import { MemoryCredentials } from "./MemoryCredentials";
import { MemoryWatchers } from "./MemoryWatchers";
import { MemoryActivity } from "./MemoryActivity";

/**
 * 🍓 Memory — the entry point for the generic memory area.
 *
 * The sidebar contains 7 sections. We track the active sub-section
 * in URL-style state. The overview is the default.
 */
export function MemoryPanel() {
  const nav = useMemoryNav();
  const [memory, setMemory] = useState<Memory | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const refresh = () => setRefreshKey((k) => k + 1);

  // When the URL points to a specific memory, load it.
  useEffect(() => {
    if (nav.view === "detail" && nav.memoryId) {
      api.memoryGet(nav.memoryId)
        .then((m) => setMemory(m))
        .catch(() => setMemory(null));
    } else {
      setMemory(null);
    }
  }, [nav.view, nav.memoryId, refreshKey]);

  // Listen for global "open memory" events (e.g. from DOCX BlockLinkPanel).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ memoryId: string }>).detail;
      if (detail?.memoryId) {
        nav.goDetail(detail.memoryId);
      }
    };
    window.addEventListener("strawberry:open-memory", handler);
    return () => window.removeEventListener("strawberry:open-memory", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="memory-panel">
      <div className="memory-subnav" role="tablist" aria-label="Memory sections">
        <SubNavButton
          label="Overview"
          icon="▦"
          active={nav.view === "overview"}
          onClick={() => nav.goOverview()}
        />
        <SubNavButton
          label="Search"
          icon="🔎"
          active={nav.view === "search"}
          onClick={() => nav.goSearch()}
        />
        <SubNavButton
          label="Images"
          icon="🖼️"
          active={nav.view === "images"}
          onClick={() => nav.goImages()}
        />
        <SubNavButton
          label="Credentials"
          icon="🔐"
          active={nav.view === "credentials"}
          onClick={() => nav.goCredentials()}
        />
        <SubNavButton
          label="Watchers"
          icon="📁"
          active={nav.view === "watchers"}
          onClick={() => nav.goWatchers()}
        />
        <SubNavButton
          label="Activity"
          icon="🤖"
          active={nav.view === "activity"}
          onClick={() => nav.goActivity()}
        />
        <button
          className="memory-create-btn"
          onClick={() => nav.goCreate()}
          title="Create a new memory"
        >
          + New Memory
        </button>
      </div>

      <div className="memory-content" key={refreshKey}>
        {nav.view === "overview" && <MemoryOverview onNavigate={nav} />}
        {nav.view === "search" && <MemorySearch onNavigate={nav} />}
        {nav.view === "detail" && memory && (
          <MemoryDetail memory={memory} onNavigate={nav} onChange={refresh} />
        )}
        {nav.view === "detail" && !memory && (
          <EmptyState
            title="Memory not found"
            body="The requested memory may have been deleted or never existed."
            action="Back to overview"
            onAction={() => nav.goOverview()}
          />
        )}
        {nav.view === "create" && <MemoryCreate onNavigate={nav} />}
        {nav.view === "edit" && memory && (
          <MemoryCreate memory={memory} onNavigate={nav} />
        )}
        {nav.view === "edit" && !memory && (
          <EmptyState
            title="Memory not found"
            body="Cannot edit a memory that doesn't exist."
            action="Back to overview"
            onAction={() => nav.goOverview()}
          />
        )}
        {nav.view === "images" && <MemoryImages />}
        {nav.view === "credentials" && <MemoryCredentials />}
        {nav.view === "watchers" && <MemoryWatchers />}
        {nav.view === "activity" && <MemoryActivity />}
      </div>
    </div>
  );
}

function SubNavButton({
  label,
  icon,
  active,
  onClick,
}: {
  label: string;
  icon: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={`memory-subnav-btn${active ? " active" : ""}`}
      onClick={onClick}
      role="tab"
      aria-selected={active}
    >
      <span className="memory-subnav-icon" aria-hidden>{icon}</span>
      {label}
    </button>
  );
}

function EmptyState({
  title,
  body,
  action,
  onAction,
}: {
  title: string;
  body: string;
  action: string;
  onAction: () => void;
}) {
  return (
    <div className="memory-empty">
      <h2 className="memory-empty-title">{title}</h2>
      <p className="memory-empty-body">{body}</p>
      <button className="btn" onClick={onAction}>{action}</button>
    </div>
  );
}
