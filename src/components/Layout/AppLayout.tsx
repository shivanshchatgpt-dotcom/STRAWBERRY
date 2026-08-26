import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useAppStore } from "../../store/appStore";
import { SearchBox } from "../Search/SearchBox";
import { DashboardView } from "../Dashboard/DashboardView";
import { ScreensView } from "../Screens/ScreensView";
import { InboxView } from "../Inbox/InboxView";

type View = "dashboard" | "tree" | "screens" | "inbox";

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
            <svg className="brand-mark" viewBox="0 0 24 24" aria-hidden="true">
              <defs>
                <linearGradient id="brandBerryBody" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0" stopColor="#ffffff" />
                  <stop offset="1" stopColor="#ffe4e6" />
                </linearGradient>
              </defs>
              <path
                d="M12 7.4C11 5.9 9.2 5 7.1 5.2c.9.7 1.5 1.5 1.8 2.3-1.1-.3-2.3-.1-3.3.5 1.2.4 2.1 1 2.8 1.8"
                fill="none" stroke="#6ee7b7" strokeWidth="1.6"
                strokeLinecap="round" strokeLinejoin="round"
              />
              <path
                d="M12 7.4c1-1.5 2.8-2.4 4.9-2.2-.9.7-1.5 1.5-1.8 2.3 1.1-.3 2.3-.1 3.3.5-1.2.4-2.1 1-2.8 1.8"
                fill="none" stroke="#34d399" strokeWidth="1.6"
                strokeLinecap="round" strokeLinejoin="round"
              />
              <path
                d="M12 21.8c-4.4-2.6-6.8-5.8-6.8-9 0-2.9 3-4.5 6.8-4.5s6.8 1.6 6.8 4.5c0 3.2-2.4 6.4-6.8 9z"
                fill="url(#brandBerryBody)"
              />
              <g fill="#f43f5e" opacity="0.85">
                <circle cx="9.5" cy="12.6" r="0.8" />
                <circle cx="14.5" cy="12.6" r="0.8" />
                <circle cx="12" cy="14.8" r="0.8" />
                <circle cx="10.3" cy="17.2" r="0.7" />
                <circle cx="13.7" cy="17.2" r="0.7" />
              </g>
            </svg>
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
            localStorage.setItem("strawberry-theme", next);
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
        ) : showDashboard ? (
          <DashboardView />
        ) : (
          children
        )}
      </div>
    </div>
  );
}
