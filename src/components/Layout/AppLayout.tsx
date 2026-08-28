import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useAppStore } from "../../store/appStore";
import { SearchBox } from "../Search/SearchBox";
import { DashboardView } from "../Dashboard/DashboardView";
import { ScreensView } from "../Screens/ScreensView";
import { InboxView } from "../Inbox/InboxView";
import { PlannerView } from "../Planner/PlannerView";
import { AmbientMemoryView } from "../AmbientMemory/AmbientMemoryView";
import strawberryIcon from "../../assets/strawberry-icon.png";

type View = "dashboard" | "tree" | "screens" | "inbox" | "planner" | "ambient";

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
  const goInbox = () => {
    clearSearch();
    setView("inbox");
  };
  const goPlanner = () => {
    clearSearch();
    setView("planner");
  };
  const goAmbient = () => {
    clearSearch();
    setView("ambient");
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
      } else if ((e.ctrlKey || e.metaKey) && e.key === "4") {
        e.preventDefault();
        goInbox();
      } else if ((e.ctrlKey || e.metaKey) && e.key === "5") {
        e.preventDefault();
        goPlanner();
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
          title="Strawberry — Dashboard (Ctrl+1)"
          aria-label="Strawberry dashboard"
        >
          <span className="logo">
            <img src={strawberryIcon} alt="" className="brand-img" />
          </span>
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
          <button
            className={`nav-tab${view === "inbox" ? " active" : ""}`}
            onClick={goInbox}
            title="Ctrl/Cmd+4"
          >
            📥 Inbox
          </button>
          <button
            className={`nav-tab${view === "planner" ? " active" : ""}`}
            onClick={goPlanner}
            title="Ctrl/Cmd+5"
          >
            🗓️ Planner
          </button>
          <button
            className={`nav-tab${view === "ambient" ? " active" : ""}`}
            onClick={goAmbient}
            title="Ambient Memory"
          >
            🧠 Ambient Memory
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
        <button
          className="theme-toggle"
          title="Toggle theme (dark / light)"
          aria-label="Toggle theme"
          onClick={() => {
            const next =
              document.documentElement.dataset.theme === "dark" ? "light" : "dark";
            document.documentElement.dataset.theme = next;
            localStorage.setItem("strawberry-theme-v2", next);
          }}
        >
          {document.documentElement.dataset.theme === "dark" ? "☀️" : "🌙"}
        </button>
        <span className="offline-chip" title="All data stays on this machine">
          Offline · Local
        </span>
      </header>
      <div className="main-area">
        {view === "screens" ? (
          <ScreensView />
        ) : view === "inbox" ? (
          <InboxView />
        ) : view === "planner" ? (
          <PlannerView />
        ) : view === "ambient" ? (
          <AmbientMemoryView />
        ) : showDashboard ? (
          <DashboardView />
        ) : (
          children
        )}
      </div>
    </div>
  );
}
