import { useAppStore } from "../../store/appStore";
import { EmptyState } from "../EmptyState/EmptyState";
import type { Root } from "../../lib/types";

export function HomeView() {
  const roots = useAppStore((s) => s.roots);
  const rootsLoading = useAppStore((s) => s.rootsLoading);
  const openDialog = useAppStore((s) => s.openDialog);

  return (
    <>
      <div className="content">
        <div className="home-hero">
          <div className="page-head">
            <div>
              <h1>Your Indexes</h1>
              <div className="sub">
                Top-level trees for your saved chats and knowledge.
              </div>
            </div>
            <div className="page-actions">
              <button
                className="btn primary"
                onClick={() => openDialog({ kind: "create-root" })}
                title="Ctrl/Cmd+Shift+N"
              >
                + New Index
              </button>
            </div>
          </div>

          {rootsLoading ? (
            <div className="loading-block">
              <span className="spinner" /> Loading indexes…
            </div>
          ) : roots.length === 0 ? (
            <EmptyState
              icon="🌱"
              title="Create your first index/root"
              hint="Indexes are top-level containers like School, Biology, or Vibe Coding. Inside them you can nest unlimited folders and save chats with locally generated briefs."
              actionLabel="+ Create your first index"
              onAction={() => openDialog({ kind: "create-root" })}
            />
          ) : (
            <div className="root-grid">
              {roots.map((root) => (
                <RootCard key={root.id} root={root} />
              ))}
            </div>
          )}
        </div>
      </div>
    </>
  );
}

function RootCard({ root }: { root: Root }) {
  const openRoot = useAppStore((s) => s.openRoot);
  const openDialog = useAppStore((s) => s.openDialog);

  return (
    <div
      className="root-card"
      role="group"
      aria-label={`Index ${root.name}`}
    >
      <button
        style={{ all: "unset", cursor: "pointer", display: "block", width: "100%" }}
        onClick={() => void openRoot(root.id)}
        onDoubleClick={() => void openRoot(root.id)}
        title={`Open ${root.name}`}
      >
        <span className="root-name">{root.name}</span>
        <span className="root-meta" style={{ display: "block" }}>Created {new Date(root.createdAt).toLocaleDateString()}</span>
      </button>
      <div className="row-actions">
        <button
          className="icon-btn"
          title="Open"
          onClick={() => void openRoot(root.id)}
        >
          ↗
        </button>
        <button
          className="icon-btn"
          title="Rename"
          onClick={() => openDialog({ kind: "rename-root", rootId: root.id, name: root.name })}
        >
          ✎
        </button>
        <button
          className="icon-btn danger"
          title="Delete"
          onClick={() =>
            openDialog({
              kind: "confirm-delete",
              target: { type: "root", id: root.id, name: root.name },
            })
          }
        >
          🗑
        </button>
      </div>
    </div>
  );
}
