import { useAppStore } from "../../store/appStore";

export function BriefTab({ markdown }: { markdown: string }) {
  if (!markdown.trim()) {
    return (
      <div className="text-dim">
        No brief generated yet. Press “Regenerate Brief” to run the local
        rule-based extractor.
      </div>
    );
  }
  return (
    <pre className="pre-block brief-text" aria-label="Generated brief">
      {markdown}
    </pre>
  );
}

export function useBriefBusy(): boolean {
  return useAppStore((s) => s.busy);
}
