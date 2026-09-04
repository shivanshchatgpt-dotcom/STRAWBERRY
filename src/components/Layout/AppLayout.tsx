import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { useAppStore } from "../../store/appStore";
import { SearchBox } from "../Search/SearchBox";
import { DashboardView } from "../Dashboard/DashboardView";
import { ScreensView } from "../Screens/ScreensView";
import { InboxView } from "../Inbox/InboxView";
import { PlannerView } from "../Planner/PlannerView";
import { CalendarView } from "../Calendar/CalendarView";
import { AmbientMemoryView } from "../AmbientMemory/AmbientMemoryView";
import { GhostPanel } from "../Ghost/GhostPanel";
import { AutonomyPanel } from "../Autonomy/AutonomyPanel";
import { HealthLensView } from "../Health/HealthLensView";
import { StoryExportView } from "../Story/StoryExportView";
import { DatabaseView } from "../Database/DatabaseView";
import { DocxView } from "../Docx/DocxView";
import { ProjectsView } from "../Projects/ProjectsView";
import { LeftSidebar } from "./LeftSidebar";
import { RightSidebar } from "./RightSidebar";
import { WellnessOverlayHost } from "../Wellness/WellnessOverlayHost";
import strawberryIcon from "../../assets/strawberry-icon.png";
import type { View } from "./viewTypes";

/**
 * 🍓 AppLayout — 3-window shell with a fixed background image.
 *
 *   ┌────────────────────────────────────────────┐
 *   │ topbar (brand · search · actions)          │
 *   ├──────────┬───────────────────┬────────────┤
 *   │ left     │ center             │ right      │
 *   │ nav rail │ real working       │ habits +   │
 *   │          │ project widgets    │ calendar   │
 *   └──────────┴───────────────────┴────────────┘
 *
 * The center pane renders the project's real working components
 * (ResumeBanner, ContextRecall, Wellness, Alpha Hunter, briefing,
 * todos, habits, charts, health lens, story export) — not mock widgets.
 */
export function AppLayout({ children }: { children: ReactNode }) {
  const openDialog = useAppStore((s) => s.openDialog);
  const currentRootId = useAppStore((s) => s.currentRootId);
  const currentChatId = useAppStore((s) => s.currentChatId);
  const searchResults = useAppStore((s) => s.searchResults);
  const goHome = useAppStore((s) => s.goHome);
  const clearSearch = useAppStore((s) => s.clearSearch);

  const [view, setView] = useState<View>("dashboard");
  const inDetailView = Boolean(currentRootId) || Boolean(currentChatId) || searchResults !== null;

  const navigate = (v: View) => {
    if (v === "dashboard") {
      clearSearch();
      goHome();
      setView("dashboard");
      return;
    }
    if (v === "settings") {
      openDialog({ kind: "settings" });
      return;
    }
    clearSearch();
    setView(v);
  };

  // Keyboard: Ctrl/Cmd+1..5 for the first five views.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const map: Record<string, View> = {
        "1": "dashboard",
        "2": "progress",
        "3": "system_scan",
        "4": "outbox",
        "5": "calendar",
      };
      const v = map[e.key];
      if (v) {
        e.preventDefault();
        navigate(v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const [theme, setTheme] = useState(
    () => document.documentElement.dataset.theme || "light",
  );

  const toggleTheme = () => {
    const next = theme === "dark" ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    localStorage.setItem("strawberry-theme-v3", next);
    setTheme(next);
  };

  const centerView = (() => {
    // Knowledge view always shows the real tree browser (HomeView with the
    // saved roots, or BrowserView once a root is open). This is where all
    // saved content lives — it must never be shadowed by the dashboard.
    if (view === "progress") return children;
    if (inDetailView) return children;
    switch (view) {
      case "system_scan": return <ScreensView />;
      case "calendar":    return <CalendarView />;
      case "outbox":      return <InboxView />;
      case "planner":     return <PlannerView />;
      case "ambient":     return <AmbientMemoryView />;
      case "ghost":       return <GhostPanel />;
      case "autonomy":    return <AutonomyPanel />;
      case "health":      return <HealthLensView />;
      case "story":       return <StoryExportView />;
      case "database":    return <DatabaseView />;
      case "docx":        return <DocxView />;
      case "projects":    return <ProjectsView />;
      default:            return <DashboardView />;
    }
  })();

  return (
    <div className="app-shell">
      <div className="bg-image" aria-hidden />
      <div className="bg-veil" aria-hidden />

      {/* In-app wellness reminder popup — always visible even if the
          OS-level popup window fails (e.g. some Wayland compositors). */}
      <WellnessOverlayHost />

      <header className="topbar">
        <button
          className="brand"
          onClick={() => navigate("dashboard")}
          title="Strawberry — Dashboard (Ctrl+1)"
          aria-label="Strawberry dashboard"
        >
          <span className="logo">
            <img src={strawberryIcon} alt="" className="brand-img" />
          </span>
        </button>

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
          onClick={toggleTheme}
        >
          {theme === "dark" ? "☀️" : "🌙"}
        </button>
        <span className="offline-chip" title="All data stays on this machine">
          Offline · Local
        </span>
      </header>

      <div className="main-area">
        <LeftSidebar active={view} onNavigate={navigate} />
        <main className="center-pane">{centerView}</main>
        <RightSidebar />
      </div>
    </div>
  );
}
