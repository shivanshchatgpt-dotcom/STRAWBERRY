import { useAppStore } from "../../store/appStore";
import { formatDate } from "../../lib/utils";

export function MetadataTab() {
  const detail = useAppStore((s) => s.chatDetail);
  const rootName = useAppStore((s) =>
    s.roots.find((r) => r.id === s.chatDetail?.meta.rootId)?.name,
  );
  const breadcrumb = useAppStore((s) => s.breadcrumb);
  if (!detail) return null;
  const m = detail.meta;
  const folderPath = breadcrumb
    .filter((b) => b.kind === "folder")
    .map((b) => b.label)
    .join(" / ");

  return (
    <div>
      <dl className="meta-table">
        <dt>Title</dt>
        <dd>{m.title}</dd>
        <dt>Root</dt>
        <dd>{rootName ?? m.rootId}</dd>
        <dt>Folder path</dt>
        <dd>{folderPath || "(index root)"}</dd>
        <dt>Source</dt>
        <dd>{m.source}</dd>
        <dt>Tags</dt>
        <dd>
          {m.tags
            ? m.tags
                .split(",")
                .map((t) => t.trim())
                .filter(Boolean)
                .map((t) => (
                  <span key={t} className="tag-chip">
                    {t}
                  </span>
                ))
            : "—"}
        </dd>
        <dt>First idea</dt>
        <dd>{m.firstIdea ?? "—"}</dd>
        <dt>Created</dt>
        <dd>{formatDate(m.createdAt)}</dd>
        <dt>Updated</dt>
        <dd>{formatDate(m.updatedAt)}</dd>
        <dt>Original file</dt>
        <dd className="mono">{m.rawPath}</dd>
        <dt>Brief file</dt>
        <dd className="mono">{m.briefPath ?? "—"}</dd>
        <dt>Chat ID</dt>
        <dd className="mono">{m.chatId}</dd>
        <dt>Node ID</dt>
        <dd className="mono">{m.nodeId}</dd>
      </dl>

      <div className="stat-grid">
        <StatBox label="Characters" value={m.stats.charCount} />
        <StatBox label="Words" value={m.stats.wordCount} />
        <StatBox label="Code blocks" value={m.stats.codeBlockCount} />
        <StatBox label="Errors" value={m.stats.errorCount} />
        <StatBox label="Commands" value={m.stats.commandCount} />
        <StatBox label="URLs" value={m.stats.urlCount} />
      </div>

      <p className="text-dim mt-16" style={{ fontSize: 12.5 }}>
        Files live under the app data directory using stable IDs, so renaming
        items never breaks storage. Deleting this chat removes the folder above.
      </p>
    </div>
  );
}

function StatBox({ label, value }: { label: string; value: number }) {
  return (
    <div className="stat-box">
      <div className="stat-value">{value.toLocaleString()}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}
