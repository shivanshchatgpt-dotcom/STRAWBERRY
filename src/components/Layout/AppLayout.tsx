import type { ReactNode } from "react";
import { useAppStore } from "../../store/appStore";
import { SearchBox } from "../Search/SearchBox";

export function AppLayout({ children }: { children: ReactNode }) {
  const goHome = useAppStore((s) => s.goHome);
  const openDialog = useAppStore((s) => s.openDialog);
  const currentRootId = useAppStore((s) => s.currentRootId);

  return (
    <div className="app-shell">
      <header className="topbar">
        <button className="brand" onClick={goHome} title="Home">
          <span className="logo">▲</span>
          Chat Memory Tree
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
        <span className="offline-chip" title="All data stays on this machine">
          ● Offline · Local
        </span>
      </header>
      <div className="main-area">{children}</div>
    </div>
  );
}
