import { useEffect } from "react";
import { useAppStore } from "../../store/appStore";

interface Props {
  icon?: string;
  title: string;
  hint?: string;
  actionLabel?: string;
  onAction?: () => void;
}

export function EmptyState({ icon, title, hint, actionLabel, onAction }: Props) {
  return (
    <div className="empty-state">
      {icon && <div className="empty-icon" aria-hidden>{icon}</div>}
      <h3>{title}</h3>
      {hint && <p>{hint}</p>}
      {actionLabel && onAction && (
        <button
          className="btn primary"
          onClick={onAction}
          autoFocus={useAppStore.getState().dialog.kind === "none"}
        >
          {actionLabel}
        </button>
      )}
    </div>
  );
}

/** Full-page variant shown while the very first data load is in flight. */
export function AppLoadingGate() {
  const rootsLoading = useAppStore((s) => s.rootsLoading);
  useEffect(() => {
    // no-op; hook exists so callers can render conditionally
  }, [rootsLoading]);
  if (!rootsLoading) return null;
  return (
    <div className="loading-block">
      <span className="spinner" /> Loading…
    </div>
  );
}
