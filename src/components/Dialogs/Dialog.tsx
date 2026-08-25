import { useEffect, useRef } from "react";
import type { FormEvent, ReactNode } from "react";
import { useAppStore } from "../../store/appStore";

interface Props {
  title: string;
  hint?: string;
  submitLabel: string;
  busy: boolean;
  canSubmit: boolean;
  onSubmit: () => void;
  children?: ReactNode;
}

export function Dialog({
  title,
  hint,
  submitLabel,
  busy,
  canSubmit,
  onSubmit,
  children,
}: Props) {
  const closeDialog = useAppStore((s) => s.closeDialog);
  const firstFieldRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    firstFieldRef.current?.focus();
  }, []);

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (canSubmit && !busy) onSubmit();
  };

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeDialog();
      }}
    >
      <div className="dialog" role="dialog" aria-modal="true" aria-label={title}>
        <h3>{title}</h3>
        {hint && <p className="dialog-hint">{hint}</p>}
        <form onSubmit={handleSubmit}>
          {children}
          <div className="dialog-actions">
            <button
              type="button"
              className="btn ghost"
              onClick={closeDialog}
              disabled={busy}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn primary"
              disabled={!canSubmit || busy}
            >
              {busy ? <span className="spinner" /> : null}
              {submitLabel}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
