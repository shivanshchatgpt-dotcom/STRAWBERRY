import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useAppStore } from "../../store/appStore";
import { SearchBox } from "../Search/SearchBox";
import { DashboardView } from "../Dashboard/DashboardView";

type View = "dashboard" | "tree";

export function AppLayout({ children }: { children: ReactNode }) {
  const openDialog = useAppStore((s) => s.openDialog);
  const currentRootId = useAppStore((s) => s.currentRootId);
  const currentChatId = useAppStore((s) => s.currentChatId);
  const searchResults = useAppStore((s) => s.searchResults);

  // Dashboard vs tree navigation. A specific root/chat view or active search
  // always wins over the dashboard toggle.
  const [view, setView] = useState<View>("dashboard");
  const inDetailView = Boolean(currentRootId) || Boolean(currentChatId) || searchResults !== null;
  const showDashboard = view === "dashboard" && !inDetailView;

  // Keyboard: Ctrl/Cmd+1 dashboard, Ctrl/Cmd+2 tree.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "1") {
        e.preventDefault();
        setView("dashboard");
      } else if ((e.ctrlKey || e.metaKey) && e.key === "2") {
        e.preventDefault();
        setView("tree");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app-shell">
      <header className="topbar">
        <button
          className="brand"
          onClick={() => setView("dashboard")}
          title="Dashboard"
        >
          <span className="logo">🍓</span>
          Strawberry
        </button>

        <nav className="nav-tabs" aria-label="Main views">
          <button
            className={`nav-tab${showDashboard ? " active" : ""}`}
            onClick={() => setView("dashboard")}
            title="Ctrl/Cmd+1"
          >
            ⌂ Dashboard
          </button>
          <button
            className={`nav-tab${!showDashboard ? " active" : ""}`}
            onClick={() => setView("tree")}
            title="Ctrl/Cmd+2"
          >
            🌳 Knowledge Tree
          </button>
        </nav>

        <SearchBox />
        <div className="topbar-spacer" />
        {!currentRootId && (
          <button
            className="btn"
            onClick={() => openDialog({ kind: "create-root" })}
            title="Ctrl/Cmd+Shift+N"
          >
            + New Index
          </button>
        )}
        <span className="offline-chip" title="All data stays on this machine">
          ● Offline · Local
        </span>
      </header>
      <div className="main-area">
        {showDashboard ? <DashboardView /> : children}
      </div>
    </div>
  );
}
