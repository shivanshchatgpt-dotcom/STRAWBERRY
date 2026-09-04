import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/**
 * 🍦 WellnessReminderOverlay — in-app top-center popup.
 *
 * The Rust WellnessAgent emits `wellness:popup` every time a reminder is due.
 * The popup WINDOW (WellnessPopup.tsx) still handles the OS-level window,
 * but on compositors where transparent frameless windows fail silently
 * (some Wayland setups), this in-app overlay guarantees the reminder is
 * always visible inside the main Strawberry window.
 */

type Reminder = {
  category: string;
  title: string;
  message: string;
  emoji: string;
  durationSecs: number;
};

export function WellnessOverlayHost() {
  const [reminder, setReminder] = useState<Reminder | null>(null);
  const [pct, setPct] = useState(100);
  const hideTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    listen<Reminder>("wellness:popup", (ev) => {
      setReminder(ev.payload);
      setPct(100);
    }).then((u) => { dispose = u; });
    return () => { dispose?.(); };
  }, []);

  useEffect(() => {
    if (!reminder) return;
    // Clamp duration between 3 and 8 seconds (same policy as the popup window).
    const duration = Math.max(3, Math.min(8, reminder.durationSecs)) * 1000;
    const start = Date.now();

    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setReminder(null), duration);

    let raf = 0;
    const tick = () => {
      const elapsed = Date.now() - start;
      const remaining = Math.max(0, duration - elapsed);
      setPct((remaining / duration) * 100);
      if (remaining > 0) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    return () => {
      window.clearTimeout(hideTimer.current);
      cancelAnimationFrame(raf);
    };
  }, [reminder]);

  if (!reminder) return null;

  return (
    <div className="wellness-overlay" role="alert" aria-live="assertive">
      <div className="wellness-overlay-card">
        <div className="wellness-overlay-row">
          <span className="wellness-overlay-emoji">{reminder.emoji}</span>
          <div className="wellness-overlay-text">
            <div className="wellness-overlay-title">{reminder.title}</div>
            <div className="wellness-overlay-message">{reminder.message}</div>
          </div>
          <button
            className="wellness-overlay-close"
            aria-label="Dismiss"
            onClick={() => setReminder(null)}
          >
            ✕
          </button>
        </div>
        <div className="wellness-overlay-bar">
          <span style={{ width: `${pct}%` }} />
        </div>
      </div>
    </div>
  );
}
