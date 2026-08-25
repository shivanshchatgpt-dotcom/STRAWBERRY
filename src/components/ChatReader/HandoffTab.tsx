import { useEffect } from "react";
import { useAppStore } from "../../store/appStore";
import { EmptyState } from "../EmptyState/EmptyState";
import { SLOT_LABELS } from "../../lib/types";
import type { HandoffSlot } from "../../lib/types";

/** Budget presets covering the realistic paste sizes. */
const PRESETS = [300, 500, 700, 1200, 2000];

/**
 * Feature 1 — the AI-to-AI handoff tab.
 *
 * Shows the paste-ready packet, an auditable budget report and per-slot
 * kept/dropped counts. Everything is produced by the deterministic Rust
 * engine; this component only displays and copies it.
 */
export function HandoffTab() {
  const chatId = useAppStore((s) => s.currentChatId);
  const handoff = useAppStore((s) => s.handoff);
  const loading = useAppStore((s) => s.handoffLoading);
  const budget = useAppStore((s) => s.handoffBudget);
  const buildHandoff = useAppStore((s) => s.buildHandoff);
  const setBudget = useAppStore((s) => s.setHandoffBudget);
  const copyText = useAppStore((s) => s.copyText);

  // Build once when the tab opens, and again whenever the budget changes
  // (setHandoffBudget clears the packet, so this refills it).
  useEffect(() => {
    if (chatId && !handoff && !loading) void buildHandoff();
  }, [chatId, handoff, loading, buildHandoff]);

  if (!chatId) return null;

  if (loading && !handoff) {
    return (
      <div className="loading-block">
        <span className="spinner" /> Compressing chat…
      </div>
    );
  }

  if (!handoff) {
    return (
      <EmptyState
        icon="🍓"
        title="No handoff packet yet"
        hint="Press Rebuild to compress this chat into a packet another AI can read cold."
      />
    );
  }

  const { rendered, json, packet } = handoff;
  const b = packet.budget;

  return (
    <div className="handoff">
      <div className="handoff-controls">
        <label htmlFor="handoff-budget">Token budget</label>
        <select
          id="handoff-budget"
          value={PRESETS.includes(budget) ? budget : "custom"}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (!Number.isNaN(v)) setBudget(v);
          }}
        >
          {PRESETS.map((p) => (
            <option key={p} value={p}>
              {p} tokens
            </option>
          ))}
          {!PRESETS.includes(budget) && (
            <option value="custom">{budget} tokens</option>
          )}
        </select>

        <button
          className="btn"
          disabled={loading}
          onClick={() => void buildHandoff()}
          title="Re-run the deterministic compressor"
        >
          ↻ Rebuild
        </button>
        <button
          className="btn primary"
          onClick={() => void copyText(rendered, "Handoff packet")}
          title="Copy the paste-ready block"
        >
          📋 Copy Handoff
        </button>
        <button
          className="btn"
          onClick={() => void copyText(json, "Handoff JSON")}
          title="Copy the .strawberry.json form"
        >
          ⤓ Copy JSON
        </button>
      </div>

      <div className="handoff-stats" role="status">
        <Stat label="Original" value={`${b.originalTokens} tok`} />
        <Stat label="Packet" value={`${b.packetTokens} tok`} />
        <Stat label="Smaller by" value={`${b.reductionPct}%`} strong />
        <Stat label="Budget" value={`${b.tokenBudget} tok`} />
        {b.overBudget && (
          <span className="badge warn" title="The goal line is never dropped">
            over budget
          </span>
        )}
      </div>

      <p className="text-dim handoff-note">
        Task-lossless, not lossless: every slot needed to pick the next action
        is kept, prose is dropped, and the original chat stays untouched on
        disk. Ask for a section by name to recover detail.
      </p>

      <div className="list-separator">Paste this into the other AI</div>
      <textarea
        className="handoff-output"
        readOnly
        value={rendered}
        rows={Math.min(28, rendered.split("\n").length + 1)}
        aria-label="Handoff packet, paste-ready"
        onFocus={(e) => e.currentTarget.select()}
      />

      <div className="list-separator">What each slot kept</div>
      <table className="handoff-slots">
        <thead>
          <tr>
            <th scope="col">Slot</th>
            <th scope="col">Kept</th>
            <th scope="col">Dropped</th>
          </tr>
        </thead>
        <tbody>
          {b.slots.map((s) => (
            <tr key={s.slot}>
              <th scope="row">{SLOT_LABELS[s.slot as HandoffSlot] ?? s.slot}</th>
              <td>{s.kept}</td>
              <td className={s.dropped > 0 ? "text-dim" : undefined}>
                {s.dropped}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {packet.rejected.length > 0 && (
        <>
          <div className="list-separator">
            Rejected approaches (never re-propose these)
          </div>
          <div className="artifact-list">
            {packet.rejected.map((r, i) => (
              <div key={i} className="artifact-item">
                <strong>{r.what}</strong>
                {r.why && <div className="text-dim">why: {r.why}</div>}
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  strong,
}: {
  label: string;
  value: string;
  strong?: boolean;
}) {
  return (
    <span className="handoff-stat">
      <span className="handoff-stat-label">{label}</span>
      <span className={strong ? "handoff-stat-value strong" : "handoff-stat-value"}>
        {value}
      </span>
    </span>
  );
}
