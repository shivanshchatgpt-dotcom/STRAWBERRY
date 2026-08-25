import { useEffect, useState } from "react";
import { useAppStore } from "../../store/appStore";
import { Breadcrumb } from "../Breadcrumb/Breadcrumb";
import { TreePanel } from "../Tree/TreePanel";
import { TabBar } from "../Tabs/TabBar";
import { BriefTab } from "./BriefTab";
import { OriginalTab } from "./OriginalTab";
import { ArtifactsTab } from "./ArtifactsTab";
import { HandoffTab } from "./HandoffTab";
import { MetadataTab } from "./MetadataTab";
import type { ChatArtifact, ArtifactType } from "../../lib/types";

export interface ReaderTab {
  id: string;
  label: string;
}

const TABS: ReaderTab[] = [
  { id: "brief", label: "Brief" },
  { id: "handoff", label: "🍓 Handoff" },
  { id: "original", label: "Original" },
  { id: "code", label: "Code" },
  { id: "errors", label: "Errors" },
  { id: "commands", label: "Commands" },
  { id: "decisions", label: "Decisions" },
  { id: "identifiers", label: "Identifiers" },
  { id: "metadata", label: "Metadata" },
];

export function ChatReaderView() {
  const chatId = useAppStore((s) => s.currentChatId);
  const detail = useAppStore((s) => s.chatDetail);
  const busy = useAppStore((s) => s.busy);
  const regenerateBrief = useAppStore((s) => s.regenerateBrief);
  const openDialog = useAppStore((s) => s.openDialog);
  const [activeTab, setActiveTab] = useState("brief");

  // Re-fetch when the open chat changes without detail (e.g. first mount).
  useEffect(() => {
    if (chatId && !detail) void useAppStore.getState().openChat(chatId);
  }, [chatId, detail]);

  if (!chatId || !detail) {
    return (
      <div className="content">
        <div className="loading-block">
          <span className="spinner" /> Loading chat…
        </div>
      </div>
    );
  }

  const byType = (t: ArtifactType): ChatArtifact[] =>
    detail.artifacts.filter((a) => a.artifactType === t);

  return (
    <>
      <TreePanel />
      <div className="content">
        <Breadcrumb />
        <div className="reader-head">
          <div>
            <h2>{detail.meta.title}</h2>
            <div className="meta-line">
              source: {detail.meta.source} · saved{" "}
              {new Date(detail.meta.createdAt).toLocaleString()}
              {detail.meta.tags ? ` · tags: ${detail.meta.tags}` : ""}
            </div>
          </div>
          <div className="page-actions">
            <button
              className="btn"
              disabled={busy}
              onClick={() => void regenerateBrief()}
              title="Re-run the local rule-based brief generator"
            >
              ↻ Regenerate Brief
            </button>
            <button
              className="btn primary"
              onClick={() => {
                setActiveTab("handoff");
                void useAppStore.getState().buildHandoff();
              }}
              title="Compress this chat into a packet for another AI"
            >
              🍓 Handoff
            </button>
            <button
              className="btn"
              onClick={() =>
                openDialog({
                  kind: "rename-folder",
                  nodeId: detail.meta.nodeId,
                  name: detail.meta.title,
                })
              }
            >
              ✎ Rename
            </button>
            <button
              className="btn danger"
              onClick={() =>
                openDialog({
                  kind: "confirm-delete",
                  target: {
                    type: "chat",
                    id: detail.meta.nodeId,
                    chatId: detail.meta.chatId,
                    name: detail.meta.title,
                  },
                })
              }
            >
              🗑 Delete
            </button>
          </div>
        </div>

        <TabBar tabs={TABS} active={activeTab} onSelect={setActiveTab} />

        {activeTab === "brief" && <BriefTab markdown={detail.briefMarkdown} />}
        {activeTab === "handoff" && <HandoffTab />}
        {activeTab === "original" && (
          <OriginalTab chatId={detail.meta.chatId} />
        )}
        {activeTab === "code" && <ArtifactsTab artifacts={byType("code")} mono />}
        {activeTab === "errors" && (
          <ArtifactsTab artifacts={byType("error")} />
        )}
        {activeTab === "commands" && (
          <ArtifactsTab artifacts={byType("command")} mono />
        )}
        {activeTab === "decisions" && (
          <>
            <ArtifactsTab
              artifacts={byType("decision")}
              emptyText="No decision-like lines were detected."
            />
            <RejectedSection items={byType("rejected")} />
            <ActionItemsSection items={byType("action_item")} />
          </>
        )}
        {activeTab === "identifiers" && (
          <>
            <ArtifactsTab
              artifacts={byType("identifier")}
              mono
              emptyText="No env vars, ports, tables or versions were detected."
            />
            <ConstraintsSection items={byType("constraint")} />
          </>
        )}
        {activeTab === "metadata" && <MetadataTab />}
      </div>
    </>
  );
}

function ActionItemsSection({ items }: { items: ChatArtifact[] }) {
  if (items.length === 0) return null;
  return (
    <>
      <div className="list-separator">Action Items</div>
      <div className="artifact-list">
        {items.map((a) => (
          <div key={a.id} className="artifact-item">
            {a.content}
          </div>
        ))}
      </div>
    </>
  );
}

/**
 * Negative knowledge: approaches already tried and abandoned, with the reason
 * when the chat stated one. Shown next to decisions because the pair is what
 * stops a fresh reader from re-proposing a dead end.
 */
function RejectedSection({ items }: { items: ChatArtifact[] }) {
  if (items.length === 0) return null;
  return (
    <>
      <div className="list-separator">Rejected / Already Tried</div>
      <div className="artifact-list">
        {items.map((a) => (
          <div key={a.id} className="artifact-item">
            {a.content}
          </div>
        ))}
      </div>
    </>
  );
}

function ConstraintsSection({ items }: { items: ChatArtifact[] }) {
  if (items.length === 0) return null;
  return (
    <>
      <div className="list-separator">Constraints</div>
      <div className="artifact-list">
        {items.map((a) => (
          <div key={a.id} className="artifact-item">
            {a.content}
          </div>
        ))}
      </div>
    </>
  );
}
