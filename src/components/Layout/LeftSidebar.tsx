import type { View } from "./viewTypes";

/**
 * 🍓 Left rail — project's real navigation with enlarged widgets.
 * Uniform sizing matching right panel density.
 */

const NAV: Array<{ id: View; icon: string; label: string; hint: string }> = [
  { id: "dashboard", icon: "▦", label: "Dashboard",   hint: "Daily cockpit — Ctrl+1" },
  { id: "progress",  icon: "🌳", label: "Knowledge",  hint: "Saved chats tree — Ctrl+2" },
  { id: "system_scan", icon: "📺", label: "Screens",  hint: "Screen memory — Ctrl+3" },
  { id: "outbox",    icon: "📥", label: "Inbox",      hint: "Captured items — Ctrl+4" },
  { id: "calendar",  icon: "📅", label: "Calendar",   hint: "Advanced calendar" },
  { id: "ambient",   icon: "🧠", label: "Ambient",    hint: "Ambient memory" },
  { id: "projects",  icon: "🌳", label: "Projects",   hint: "Project Brain · What Changed · Resume" },
  { id: "ghost",     icon: "👻", label: "Ghost",      hint: "Knowledge graph" },
  { id: "autonomy",  icon: "🤖", label: "Autonomy",   hint: "Runtime control" },
  { id: "health",    icon: "🩺", label: "Health Lens", hint: "Disk & cache scan" },
  { id: "story",     icon: "📖", label: "My Story",   hint: "Export narrative" },
  { id: "database",  icon: "🗄️", label: "Database",   hint: "Everything saved — live counts" },
  { id: "docx",      icon: "📄", label: "DOCX",        hint: "Offline paste-to-blocks document workspace" },
  { id: "memory",    icon: "🧠", label: "Memory",     hint: "Generic memory — search, timeline, relationships" },
];

export function LeftSidebar({
  active,
  onNavigate,
}: {
  view?: View;
  active: View;
  onNavigate: (v: View) => void;
}) {
  return (
    <aside className="left-rail" aria-label="Main navigation">
      <div className="left-rail-top">
        <button
          className="left-brand"
          onClick={() => onNavigate("dashboard")}
          title="Strawberry — Dashboard (Ctrl+1)"
          aria-label="Strawberry dashboard"
        >
          <span className="left-brand-emoji" aria-hidden>🍓</span>
          <span className="left-brand-name">Strawberry</span>
          <span className="left-brand-burger" aria-hidden>≡</span>
        </button>
      </div>

      <div className="left-rail-inner">
        {NAV.map((n) => {
          const isActive = n.id === active;
          return (
            <button
              key={n.id}
              className={`rail-nav${isActive ? " active" : ""}`}
              onClick={() => onNavigate(n.id)}
              title={n.hint}
            >
              <span className="rail-nav-icon" aria-hidden>{n.icon}</span>
              <span className="rail-nav-label">{n.label}</span>
            </button>
          );
        })}
      </div>

      <div className="left-rail-bottom">
        <button
          className={`rail-nav rail-nav-settings${active === "settings" ? " active" : ""}`}
          onClick={() => onNavigate("settings")}
          title="Settings"
        >
          <span className="rail-nav-icon" aria-hidden>⚙</span>
          <span className="rail-nav-label">Settings</span>
        </button>
        <div className="left-rail-foot" title="Strawberry v1.0.0">
          <span className="left-foot-brand">
            <span className="left-foot-emoji" aria-hidden>🍓</span>
            <span className="left-foot-text">
              <strong>Strawberry</strong>
              <small>v 1.0.0</small>
            </span>
            <span className="left-foot-caret" aria-hidden>⌄</span>
          </span>
        </div>
      </div>
    </aside>
  );
}
