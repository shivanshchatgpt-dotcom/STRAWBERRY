import { useEffect, useState } from "react";
import { api } from "../../lib/api";

const CHUNK = 40_000;

/**
 * Original chat text, fetched on demand and rendered in chunks so very large
 * files never freeze the window. The raw file on disk is never modified.
 */
export function OriginalTab({ chatId }: { chatId: string }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [visible, setVisible] = useState(CHUNK);

  useEffect(() => {
    let cancelled = false;
    setText(null);
    setError(null);
    setVisible(CHUNK);
    api
      .getChatRaw(chatId)
      .then((t) => {
        if (!cancelled) setText(t);
      })
      .catch((e) => {
        if (!cancelled) setError(typeof e === "string" ? e : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [chatId]);

  if (error) {
    return <div className="text-dim">Failed to load original text: {error}</div>;
  }
  if (text === null) {
    return (
      <div className="loading-block">
        <span className="spinner" /> Loading original…
      </div>
    );
  }
  if (text === "") {
    return <div className="text-dim">Original text is empty.</div>;
  }

  const shown = text.slice(0, visible);
  const remaining = Math.max(0, text.length - visible);

  return (
    <>
      <div className="artifact-count">
        Showing {shown.length.toLocaleString()} of{" "}
        {text.length.toLocaleString()} characters
        {remaining > 0 ? " — load more below" : ""}
      </div>
      <pre className="pre-block original-text" aria-label="Original chat">
        {shown}
      </pre>
      {remaining > 0 && (
        <div className="load-more-wrap">
          <button className="btn" onClick={() => setVisible((v) => v + CHUNK)}>
            Load more ({Math.min(remaining, CHUNK).toLocaleString()} chars)
          </button>
        </div>
      )}
    </>
  );
}
