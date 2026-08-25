import { EmptyState } from "../EmptyState/EmptyState";
import type { ChatArtifact } from "../../lib/types";

interface Props {
  artifacts: ChatArtifact[];
  mono?: boolean;
  emptyText?: string;
}

/**
 * Renders a list of extracted artifacts. Code/command artifacts keep their
 * formatting in monospace; the content of code artifacts is already a full
 * fenced block, rendered verbatim.
 */
export function ArtifactsTab({ artifacts, mono, emptyText }: Props) {
  if (artifacts.length === 0) {
    return (
      <EmptyState
        icon="🗂"
        title={emptyText ?? "Nothing extracted here"}
        hint="Artifacts come from the deterministic rule-based extractor. Regenerate the brief if you recently edited the source."
      />
    );
  }

  return (
    <>
      <div className="artifact-count">
        {artifacts.length} item(s)
      </div>
      <div className="artifact-list">
        {artifacts.map((a) => (
          <div key={a.id} className={`artifact-item${mono ? " mono" : ""}`}>
            <pre>{a.content}</pre>
          </div>
        ))}
      </div>
    </>
  );
}
