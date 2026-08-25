import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { resume } from "../../lib/api";
import { useAppStore } from "../../store/appStore";

/**
 * ⏯️ ResumeBanner — "Kal tu yahan tak pahuncha tha — continue?"
 * Shows the top unfinished work with open items; dismissible.
 */
export function ResumeBanner() {
  const [points, setPoints] = useState<resume.ResumePoint[] | null>(null);
  const [day, setDay] = useState<resume.DaySummary | null>(null);
  const [showFull, setShowFull] = useState(false);
  const [dismissed, setDismissed] = useState<Set<string>>(new Set());
  const openChat = useAppStore((s) => s.openChat);

  useEffect(() => {
    void api.getResumeSuggestions(3).then(setPoints).catch(() => setPoints([]));
    void api.getDaySummary().then(setDay).catch(() => setDay(null));
  }, []);

  if (points === null || points.length === 0) return null;

  const visible = points.filter((p) => !dismissed.has(p.id));
  if (visible.length === 0) return null;
  const top = visible[0];

  const dismiss = () => {
    setDismissed((prev) => new Set(prev).add(top.id));
    void api.dismissResumePoint(top.id);
  };

  return (
    <section className="resume-banner" aria-label="Resume where you left off">
      <div className="resume-head">
        <span className="resume-badge">⏯️ Continue kahan se?</span>
        <button className="icon-btn resume-dismiss" title="Dismiss" onClick={dismiss}>
          ✕
        </button>
      </div>

      <h4 className="resume-intent">{top.intent}</h4>
      {top.chatTitle && top.chatId && (
        <button
          className="resume-chat-link"
          onClick={() => {
            clearSearchAndOpen(openChat, top.chatId!);
          }}
        >
          📄 {top.chatTitle}
        </button>
      )}

      {top.lastExchange && (
        <p className="resume-last">…{top.lastExchange.slice(0, 180)}</p>
      )}

      {top.openItems.length > 0 && (
        <ul className="resume-items">
          {top.openItems.slice(0, 3).map((item, i) => (
            <li key={i}>☐ {item}</li>
          ))}
        </ul>
      )}

      <button className="btn primary resume-day-btn" onClick={() => setShowFull((v) => !v)}>
        {showFull ? "▲ Collapse" : "🍓 Resume My Day"}
      </button>

      {showFull && day && (
        <div className="resume-day-grid">
          <div>
            <h5>Last chats</h5>
            <ul>
              {day.lastChats.map(([t, at], i) => (
                <li key={i}>📄 {t} <span className="dim">· {at.slice(0, 10)}</span></li>
              ))}
              {day.lastChats.length === 0 && <li className="dim">—</li>}
            </ul>
          </div>
          <div>
            <h5>Last captures</h5>
            <ul>
              {day.lastCaptures.map(([t, at], i) => (
                <li key={i}>📋 {t}… <span className="dim">· {at.slice(0, 10)}</span></li>
              ))}
              {day.lastCaptures.length === 0 && <li className="dim">—</li>}
            </ul>
          </div>
          <div>
            <h5>Open tasks</h5>
            <ul>
              {day.openTasks.map((t, i) => (
                <li key={i}>☐ {t}</li>
              ))}
              {day.openTasks.length === 0 && <li className="dim">—</li>}
            </ul>
          </div>
        </div>
      )}

      {visible.length > 1 && (
        <div className="resume-more text-dim">+{visible.length - 1} aur resume points</div>
      )}
    </section>
  );
}

async function clearSearchAndOpen(
  openChat: (id: string) => Promise<void>,
  chatId: string,
) {
  useAppStore.getState().clearSearch();
  await openChat(chatId);
}
