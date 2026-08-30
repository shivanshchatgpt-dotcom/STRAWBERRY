import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Reminder = {
  category: string;
  title: string;
  message: string;
  emoji: string;
  durationSecs: number;
};

const PAYLOAD_KEY = "strawberry.wellness.payload";

function readPayloadFromStorage(): Reminder | null {
  try {
    const raw = localStorage.getItem(PAYLOAD_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as Reminder;
  } catch {
    return null;
  }
}

export default function WellnessPopup() {
  // Read the payload synchronously on first render. This guarantees we
  // never miss a reminder due to a race between window-show and event-listen.
  const [reminder, setReminder] = useState<Reminder | null>(() => {
    const r = readPayloadFromStorage();
    if (r) localStorage.removeItem(PAYLOAD_KEY);
    return r;
  });
  const [pct, setPct] = useState(100);

  useEffect(() => {
    if (reminder) return;
    // Fallback: if localStorage was empty (e.g. user opened the popup
    // manually), still listen for any subsequent emit.
    let dispose: (() => void) | undefined;
    listen<Reminder>("wellness:popup", (ev) => {
      setReminder(ev.payload);
    }).then((u) => { dispose = u; });
    return () => { dispose?.(); };
  }, [reminder]);

  useEffect(() => {
    if (!reminder) return;
    // Clamp duration between 3 and 8 seconds to prevent runaway windows.
    const duration = Math.max(3, Math.min(8, reminder.durationSecs)) * 1000;
    const start = Date.now();
    const timer = setTimeout(async () => {
      try {
        // Only close the popup window. Guard against closing the main
        // window if the user navigated this popup to the main route.
        const w = getCurrentWindow();
        const label = w.label;
        if (label && label.startsWith("wellness-popup-")) {
          await w.close();
        }
      } catch {
        // ignore
      }
    }, duration + 500);

    const tick = () => {
      const elapsed = Date.now() - start;
      const remaining = Math.max(0, duration - elapsed);
      setPct((remaining / duration) * 100);
      if (remaining > 0) {
        requestAnimationFrame(tick);
      }
    };
    const id = requestAnimationFrame(tick);

    return () => {
      clearTimeout(timer);
      cancelAnimationFrame(id);
    };
  }, [reminder]);

  if (!reminder) {
    // Render a transparent placeholder so the popup window doesn't appear
    // empty / white. The window will be auto-closed by the parent if no
    // payload arrives within a few seconds (handled in main.tsx).
    return (
      <div
        style={{
          position: "fixed",
          inset: 0,
          background: "transparent",
          pointerEvents: "none",
        }}
      />
    );
  }

  return (
    <div style={{
      position: "fixed",
      inset: 0,
      display: "flex",
      justifyContent: "center",
      paddingTop: 18,
      pointerEvents: "none",
      zIndex: 99999,
    }}>
      <div style={{
        width: 380,
        padding: "14px 18px",
        borderRadius: 16,
        background: "rgba(20, 20, 20, 0.92)",
        backdropFilter: "blur(18px)",
        WebkitBackdropFilter: "blur(18px)",
        border: "1px solid rgba(255,255,255,0.18)",
        boxShadow: "0 10px 40px rgba(0,0,0,0.45)",
        color: "#fff",
        fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <span style={{ fontSize: 28 }}>{reminder.emoji}</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 15, fontWeight: 700, letterSpacing: 0.2 }}>{reminder.title}</div>
            <div style={{ fontSize: 13, opacity: 0.85, marginTop: 2 }}>{reminder.message}</div>
          </div>
        </div>
        <div style={{
          marginTop: 10,
          height: 3,
          borderRadius: 3,
          background: "rgba(255,255,255,0.15)",
          overflow: "hidden",
        }}>
          <div style={{
            height: "100%",
            width: `${pct}%`,
            borderRadius: 3,
            background: "linear-gradient(90deg, #fb7185, #e11d48)",
            transition: "width 0.1s linear",
          }} />
        </div>
      </div>
    </div>
  );
}
