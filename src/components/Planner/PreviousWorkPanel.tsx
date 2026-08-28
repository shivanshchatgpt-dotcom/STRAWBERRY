import React, { useState } from "react";
import { api } from "../../lib/api";
import type { WorkspaceItem, WorkspaceSession } from "../../lib/types";
import { useAppStore } from "../../store/appStore";

export function WorkspaceItemRow({
  item,
  onOpen,
  onRetry,
}: {
  item: WorkspaceItem;
  onOpen: () => void;
  onRetry: () => void;
}) {
  const [confirmCmd, setConfirmCmd] = useState(false);

  const getActionLabel = (item: WorkspaceItem) => {
    switch (item.actionType) {
      case "open_vscode_project":
        return "Open in VS Code";
      case "open_vscode_file":
        return "Open File";
      case "open_terminal":
        return "Open Terminal Here";
      case "run_terminal_command":
        return "Run Command";
      case "open_url":
        return "Open Tab";
      case "open_folder":
        return "Jump to Folder";
      default:
        return "Jump";
    }
  };

  const statusBadge = (status: WorkspaceItem["restoreStatus"]) => {
    switch (status) {
      case "restored":
        return <span style={{ color: "#34d399", fontSize: 12 }}>✓ Restored</span>;
      case "launching":
        return <span style={{ color: "#38bdf8", fontSize: 12 }}>⏳ Launching...</span>;
      case "failed":
        return <span style={{ color: "#f43f5e", fontSize: 12 }}>✗ Failed</span>;
      default:
        return <span style={{ color: "var(--text-dim)", fontSize: 12 }}>• Pending</span>;
    }
  };

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "8px 12px",
        background: "rgba(255,255,255,0.02)",
        borderRadius: 8,
        margin: "4px 0",
        border: "1px solid var(--border)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, flex: 1, minWidth: 0 }}>
        <span style={{ fontSize: 16 }}>
          {item.itemType === "vscode"
            ? "💻"
            : item.itemType === "terminal"
            ? "🖥️"
            : item.itemType === "browser"
            ? "🌐"
            : "🪟"}
        </span>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontWeight: 600, fontSize: 13, truncate: true } as React.CSSProperties}>
            {item.displayLabel || item.windowTitle || item.appName}
          </div>
          {item.errorMessage && (
            <div style={{ fontSize: 11, color: "#f43f5e", marginTop: 2 }}>{item.errorMessage}</div>
          )}
        </div>
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 8, marginLeft: 12 }}>
        {statusBadge(item.restoreStatus)}

        {item.actionType === "run_terminal_command" && !confirmCmd ? (
          <button
            className="btn small"
            style={{ padding: "3px 8px", fontSize: 12 }}
            onClick={() => setConfirmCmd(true)}
          >
            Run Command
          </button>
        ) : item.actionType === "run_terminal_command" && confirmCmd ? (
          <div style={{ display: "flex", gap: 4 }}>
            <button
              className="btn small primary"
              style={{ padding: "3px 8px", fontSize: 12 }}
              onClick={() => {
                setConfirmCmd(false);
                onOpen();
              }}
            >
              Confirm Run
            </button>
            <button
              className="btn small"
              style={{ padding: "3px 6px", fontSize: 12 }}
              onClick={() => setConfirmCmd(false)}
            >
              ✕
            </button>
          </div>
        ) : item.actionType ? (
          <button
            className="btn small primary"
            style={{ padding: "3px 8px", fontSize: 12 }}
            onClick={onOpen}
          >
            {getActionLabel(item)}
          </button>
        ) : null}

        {item.restoreStatus === "failed" && (
          <button
            className="btn small danger"
            style={{ padding: "3px 8px", fontSize: 12 }}
            onClick={onRetry}
          >
            Retry
          </button>
        )}
      </div>
    </div>
  );
}

export function PreviousWorkPanel() {
  const [sessions, setSessions] = useState<WorkspaceSession[]>([]);
  const [loading, setLoading] = useState(false);
  const showToast = useAppStore((s) => s.showToast);

  const refresh = async () => {
    try {
      setSessions(await api.listWorkspaceSessions(15));
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  React.useEffect(() => {
    void refresh();
  }, []);

  const freezeNow = async () => {
    setLoading(true);
    try {
      const sess = await api.freezeWorkspace();
      showToast("success", `🧊 Frozen session "${sess.name}" (${sess.items.length} items)`);
      await refresh();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    } finally {
      setLoading(false);
    }
  };

  const resumeAll = async (sessionId: string) => {
    try {
      const sess = await api.resumeWorkspaceSession(sessionId);
      const restored = sess.items.filter((i) => i.restoreStatus === "restored").length;
      showToast(
        sess.status === "restored" ? "success" : "info",
        `▶ Resumed session "${sess.name}" (${restored}/${sess.items.length} items restored)`,
      );
      await refresh();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const openItem = async (itemId: string) => {
    try {
      const res = await api.openWorkspaceItem(itemId, true);
      if (res.success) {
        showToast("success", res.message);
      } else {
        showToast("error", res.message);
      }
      await refresh();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const retryItem = async (itemId: string) => {
    try {
      const res = await api.retryWorkspaceItem(itemId);
      if (res.success) {
        showToast("success", res.message);
      } else {
        showToast("error", res.message);
      }
      await refresh();
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const deleteSess = async (id: string) => {
    try {
      await api.deleteWorkspaceSession(id);
      await refresh();
      showToast("success", "Session deleted");
    } catch (e) {
      showToast("error", typeof e === "string" ? e : String(e));
    }
  };

  const formatTs = (ts: number) => {
    if (!ts) return "";
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  return (
    <div className="planner-body">
      <section className="panel pad freeze-hero" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div>
          <div className="section-label" style={{ fontSize: 16, fontWeight: 700 }}>🍓 Previous Work (Workspace Resume v0.1)</div>
          <p className="meta-line" style={{ marginTop: 4 }}>
            Freeze Linux development sessions — VS Code, terminal working directories, Chrome/Firefox URLs, and window geometry.
          </p>
        </div>
        <button className="btn primary big-btn" onClick={() => void freezeNow()} disabled={loading}>
          {loading ? "Capturing..." : "🧊 Freeze Now"}
        </button>
      </section>

      {sessions.length === 0 ? (
        <div className="panel pad text-dim" style={{ textAlign: "center", padding: 24 }}>
          No workspace sessions captured yet. Click "Freeze Now" to snapshot your active environment.
        </div>
      ) : (
        sessions.map((sess) => {
          const restoredCount = sess.items.filter((i) => i.restoreStatus === "restored").length;
          const failedCount = sess.items.filter((i) => i.restoreStatus === "failed").length;

          return (
            <section key={sess.id} className="panel pad" style={{ marginBottom: 16 }}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  borderBottom: "1px solid var(--border)",
                  paddingBottom: 10,
                  marginBottom: 10,
                }}
              >
                <div>
                  <div style={{ fontWeight: 700, fontSize: 15, display: "flex", alignItems: "center", gap: 8 }}>
                    {sess.name}
                    <span style={{ fontSize: 12, color: "var(--text-dim)", fontWeight: 400 }}>
                      • {formatTs(sess.createdAt)} ({sess.items.length} items)
                    </span>
                  </div>
                  {sess.metadataJson && (
                    <div style={{ fontSize: 12, color: "var(--text-dim)", marginTop: 2 }}>
                      {sess.metadataJson}
                    </div>
                  )}
                </div>

                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <button className="btn primary" onClick={() => void resumeAll(sess.id)}>
                    ▶ Resume All
                  </button>
                  <button className="btn danger small" onClick={() => void deleteSess(sess.id)}>
                    Delete
                  </button>
                </div>
              </div>

              {(restoredCount > 0 || failedCount > 0) && (
                <div style={{ fontSize: 12, marginBottom: 8, color: "var(--text-dim)" }}>
                  Restore Status: {restoredCount}/{sess.items.length} restored
                  {failedCount > 0 && <span style={{ color: "#f43f5e", marginLeft: 8 }}>({failedCount} failed)</span>}
                </div>
              )}

              <div className="item-list">
                {sess.items.map((item) => (
                  <WorkspaceItemRow
                    key={item.id}
                    item={item}
                    onOpen={() => void openItem(item.id)}
                    onRetry={() => void retryItem(item.id)}
                  />
                ))}
              </div>
            </section>
          );
        })
      )}
    </div>
  );
}
