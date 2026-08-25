import { useEffect } from "react";
import { useAppStore } from "../../store/appStore";
import type { NodeSummary } from "../../lib/types";

/**
 * Lazy sidebar tree for the current root. Children load on first expand.
 */
export function TreePanel() {
  const currentRootId = useAppStore((s) => s.currentRootId);
  const roots = useAppStore((s) => s.roots);
  const childrenCache = useAppStore((s) => s.childrenCache);
  const ensureChildren = useAppStore((s) => s.ensureChildren);

  const root = roots.find((r) => r.id === currentRootId) ?? null;
  const topNodes = currentRootId
    ? childrenCache[`${currentRootId}::`]
    : undefined;

  useEffect(() => {
    if (currentRootId) void ensureChildren(currentRootId, null);
  }, [currentRootId, ensureChildren]);

  if (!currentRootId || !root) return null;

  return (
    <aside className="sidebar">
      <div className="sidebar-section">
        <div className="sidebar-title">{root.name}</div>
        {!topNodes ? (
          <div className="loading-block" style={{ padding: "8px 6px" }}>
            <span className="spinner" />
          </div>
        ) : (
          <div role="tree" aria-label={`${root.name} contents`}>
            {topNodes.map((n) => (
              <TreeRow key={n.id} node={n} depth={0} />
            ))}
            {topNodes.length === 0 && (
              <div className="text-dim" style={{ fontSize: 12.5, padding: "2px 8px" }}>
                Empty index
              </div>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}

function TreeRow({ node, depth }: { node: NodeSummary; depth: number }) {
  const expanded = useAppStore((s) => s.expandedNodeIds.has(node.id));
  const children = useAppStore(
    (s) => s.childrenCache[`${node.rootId}::${node.id}`],
  );
  const expandedNodeIds = useAppStore((s) => s.expandedNodeIds);
  const currentNodeId = useAppStore((s) => s.currentNodeId);
  const chatDetail = useAppStore((s) => s.chatDetail);
  const toggleNode = useAppStore((s) => s.toggleNode);
  const openFolder = useAppStore((s) => s.openFolder);
  const openChat = useAppStore((s) => s.openChat);

  if (node.type === "folder") {
    const isActive = currentNodeId === node.id && !chatDetail;
    const kids = expanded ? children : undefined;
    return (
      <div>
        <button
          role="treeitem"
          aria-expanded={expanded}
          className={`tree-node${isActive ? " active" : ""}`}
          style={{ paddingLeft: 6 + depth * 12 }}
          onClick={() => {
            if (!expandedNodeIds.has(node.id)) void toggleNode(node.id);
            void openFolder(node.id);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowRight" && !expanded) void toggleNode(node.id);
            if (e.key === "ArrowLeft" && expanded) void toggleNode(node.id);
          }}
        >
          <span className="caret">{expanded ? "▾" : "▸"}</span>
          <span aria-hidden>📁</span>
          <span className="node-label">{node.name}</span>
        </button>
        {kids && (
          <div className="tree-children" role="group">
            {kids.map((c) => (
              <TreeRow key={c.id} node={c} depth={depth + 1} />
            ))}
            {kids.length === 0 && (
              <div className="text-dim" style={{ fontSize: 12, padding: "1px 8px" }}>
                empty
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  const isActiveChat = chatDetail?.meta.nodeId === node.id;

  return (
    <button
      role="treeitem"
      className={`tree-node${isActiveChat ? " active" : ""}`}
      style={{ paddingLeft: 6 + depth * 12 + 16 }}
      onClick={() => {
        if (node.chatId) void openChat(node.chatId);
      }}
      title={`Open ${node.name}`}
    >
      <span className="caret">·</span>
      <span aria-hidden>💬</span>
      <span className="node-label">{node.name}</span>
    </button>
  );
}
