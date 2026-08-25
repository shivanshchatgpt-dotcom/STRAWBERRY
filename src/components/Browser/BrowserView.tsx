import { useAppStore } from "../../store/appStore";
import { Breadcrumb } from "../Breadcrumb/Breadcrumb";
import { EmptyState } from "../EmptyState/EmptyState";
import { TreePanel } from "../Tree/TreePanel";
import { formatDate } from "../../lib/utils";
import type { NodeSummary } from "../../lib/types";

export function BrowserView() {
  const currentRootId = useAppStore((s) => s.currentRootId);
  const currentNodeId = useAppStore((s) => s.currentNodeId);
  const childrenCache = useAppStore((s) => s.childrenCache);
  const breadcrumb = useAppStore((s) => s.breadcrumb);
  const roots = useAppStore((s) => s.roots);
  const openFolder = useAppStore((s) => s.openFolder);
  const openChat = useAppStore((s) => s.openChat);
  const openDialog = useAppStore((s) => s.openDialog);

  if (!currentRootId) return null;

  const key = `${currentRootId}::${currentNodeId ?? ""}`;
  const children: NodeSummary[] | undefined = childrenCache[key];
  const rootName = roots.find((r) => r.id === currentRootId)?.name ?? "";
  const currentName =
    breadcrumb[breadcrumb.length - 1]?.label ?? rootName;

  const folders = children?.filter((n) => n.type === "folder") ?? [];
  const chats = children?.filter((n) => n.type === "chat") ?? [];

  return (
    <>
      <TreePanel />
      <div className="content">
        <Breadcrumb />
        <div className="page-head browser-head">
          <div>
            <h2>{currentName}</h2>
            <div className="sub">
              {folders.length} folder(s) · {chats.length} chat(s)
            </div>
          </div>
          <div className="page-actions">
            <button
              className="btn"
              onClick={() =>
                openDialog({ kind: "create-folder", parentId: currentNodeId })
              }
              title="Ctrl/Cmd+N"
            >
              + New Folder
            </button>
            <button
              className="btn"
              onClick={() =>
                openDialog({ kind: "create-chat", parentId: currentNodeId })
              }
            >
              + New Chat
            </button>
            <button
              className="btn primary"
              onClick={() =>
                openDialog({ kind: "import-chat", parentId: currentNodeId })
              }
            >
              ⬆ Import Chat
            </button>
          </div>
        </div>

        {!children ? (
          <div className="loading-block">
            <span className="spinner" /> Loading…
          </div>
        ) : children.length === 0 ? (
          <EmptyState
            icon="📂"
            title="This folder is empty"
            hint="Create a subfolder to organize further, paste a chat as text, or import a .txt/.md/.json export."
            actionLabel="+ Add something here"
            onAction={() =>
              openDialog({ kind: "create-folder", parentId: currentNodeId })
            }
          />
        ) : (
          <div className="item-list">
            {folders.length > 0 && (
              <div className="list-separator">Folders</div>
            )}
            {folders.map((n) => (
              <ItemRow key={n.id} node={n} />
            ))}
            {chats.length > 0 && <div className="list-separator">Chats</div>}
            {chats.map((n) => (
              <ItemRow key={n.id} node={n} />
            ))}
          </div>
        )}
      </div>
    </>
  );

  function ItemRow({ node }: { node: NodeSummary }) {
    const isFolder = node.type === "folder";
    return (
      <div className={`item-row ${node.type}`}>
        <span className="item-icon" aria-hidden>
          {isFolder ? "📁" : "💬"}
        </span>
        <div
          className="item-main"
          role="button"
          tabIndex={0}
          onClick={() =>
            isFolder ? void openFolder(node.id) : node.chatId && void openChat(node.chatId)
          }
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              isFolder ? void openFolder(node.id) : node.chatId && void openChat(node.chatId);
            }
          }}
        >
          <div className="item-name">{node.name}</div>
          <div className="item-sub">
            {isFolder ? "Folder · opened by click or Enter" : `Chat · ${formatDate(node.updatedAt)}`}
          </div>
        </div>
        <div className="row-actions">
          {isFolder && (
            <>
              <button
                className="icon-btn"
                title="Rename"
                onClick={() =>
                  openDialog({ kind: "rename-folder", nodeId: node.id, name: node.name })
                }
              >
                ✎
              </button>
              <button
                className="icon-btn danger"
                title="Delete folder and everything inside it"
                onClick={() =>
                  openDialog({
                    kind: "confirm-delete",
                    target: { type: "folder", id: node.id, name: node.name },
                  })
                }
              >
                🗑
              </button>
            </>
          )}
          {!isFolder && (
            <button
              className="icon-btn danger"
              title="Delete chat"
              onClick={() =>
                node.chatId &&
                openDialog({
                  kind: "confirm-delete",
                  target: {
                    type: "chat",
                    id: node.id,
                    chatId: node.chatId,
                    name: node.name,
                  },
                })
              }
            >
              🗑
            </button>
          )}
        </div>
      </div>
    );
  }
}
