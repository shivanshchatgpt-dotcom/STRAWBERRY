import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useAppStore } from "../../store/appStore";
import { SearchBox } from "../Search/SearchBox";
import { DashboardView } from "../Dashboard/DashboardView";
import { ScreensView } from "../Screens/ScreensView";

type View = "dashboard" | "tree" | "screens";

export function AppLayout({ children }: { children: ReactNode }) {
  const openDialog = useAppStore((s) => s.openDialog);
  const currentRootId = useAppStore((s) => s.currentRootId);
  const currentChatId = useAppStore((s) => s.currentChatId);
  const searchResults = useAppStore((s) => s.searchResults);
  const goHome = useAppStore((s) => s.goHome);
  const clearSearch = useAppStore((s) => s.clearSearch);

  // Dashboard vs tree navigation. A specific root/chat view or active search
  // always wins over the dashboard toggle.
  const [view, setView] = useState<View>("dashboard");
  const inDetailView = Boolean(currentRootId) || Boolean(currentChatId) || searchResults !== null;
  const showDashboard = view === "dashboard" && !inDetailView;

  const goDashboard = () => {
    clearSearch();
    goHome();
    setView("dashboard");
  };
  const goTree = () => setView("tree");
  const goScreens = () => {
    clearSearch();
    setView("screens");
  };

  // Keyboard: Ctrl/Cmd+1 dashboard, Ctrl/Cmd+2 tree, Ctrl/Cmd+3 screens.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "1") {
        e.preventDefault();
        goDashboard();
      } else if ((e.ctrlKey || e.metaKey) && e.key === "2") {
        e.preventDefault();
        goTree();
      } else if ((e.ctrlKey || e.metaKey) && e.key === "3") {
        e.preventDefault();
        goScreens();
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
          onClick={goDashboard}
          title="Dashboard"
        >
          <span className="logo">🍓</span>
          Strawberry
        </button>

        <nav className="nav-tabs" aria-label="Main views">
          <button
            className={`nav-tab${showDashboard ? " active" : ""}`}
            onClick={goDashboard}
            title="Ctrl/Cmd+1"
          >
            ⌂ Dashboard
          </button>
          <button
            className={`nav-tab${!showDashboard && view === "tree" ? " active" : ""}`}
            onClick={goTree}
            title="Ctrl/Cmd+2"
          >
            🌳 Knowledge Tree
          </button>
          <button
            className={`nav-tab${view === "screens" ? " active" : ""}`}
            onClick={goScreens}
            title="Ctrl/Cmd+3"
          >
            📺 Screens
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
        {view === "screens" ? <ScreensView /> : showDashboard ? <DashboardView /> : children}
      </div>
    </div>
  );
}
