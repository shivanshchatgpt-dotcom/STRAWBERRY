import React from "react";
import ReactDOM from "react-dom/client";
import App, { PopupApp } from "./App";
import "./styles/global.css";

// Theme bootstrap — dark by default, persisted choice wins.
document.documentElement.dataset.theme =
  (localStorage.getItem("strawberry-theme-v2") as "dark" | "light") ?? "light";

// Route based on the window URL hash. The popup window is opened with
// "/#/wellness-popup" and should render only the small reminder — the
// main app must NOT mount in that window (otherwise it calls loadRoots,
// tries to call Tauri APIs, etc.).
const isPopup = window.location.hash === "#/wellness-popup";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
root.render(
  <React.StrictMode>
    {isPopup ? <PopupApp /> : <App />}
  </React.StrictMode>,
);

// Wellness Agent: listen for popup events in the main window only and open
// the frameless popup window. The popup window itself never installs this
// listener.
if (!isPopup) {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    // De-dupe: if a previous handler exists, detach it before installing a new one.
    const w = window as unknown as {
      __wellnessUnlisten?: () => void;
    };
    if (w.__wellnessUnlisten) {
      try { w.__wellnessUnlisten(); } catch { /* noop */ }
    }
    type Reminder = {
      category: string;
      title: string;
      message: string;
      emoji: string;
      durationSecs: number;
    };
    const unlisten = await listen<Reminder>("wellness:popup", async (ev) => {
      const payload = ev.payload;
      // Stash the payload in localStorage BEFORE the popup window mounts so
      // the popup's first render can read it. The popup clears the key on
      // mount to prevent stale replays.
      try {
        localStorage.setItem("strawberry.wellness.payload", JSON.stringify(payload));
      } catch { /* storage may be full / disabled */ }
      try {
        const label = `wellness-popup-${Date.now()}`;
        const win = new WebviewWindow(label, {
          url: "/#/wellness-popup",
          title: "🍓 Wellness",
          width: 400,
          height: 90,
          minWidth: 360,
          minHeight: 70,
          maxWidth: 500,
          maxHeight: 120,
          // Frameless + transparent can break on some Wayland compositors.
          // Use a visible (decorated) borderless-by-CSS popup as a safer
          // default; the CSS in WellnessPopup.tsx hides the chrome.
          resizable: false,
          decorations: true,
          alwaysOnTop: true,
          skipTaskbar: true,
          visible: true,
        });
        await win.show();
      } catch {
        // popup window failed; ignore
      }
    });
    w.__wellnessUnlisten = unlisten;
    window.addEventListener("beforeunload", () => {
      try { w.__wellnessUnlisten?.(); } catch { /* noop */ }
    });
  } catch {
    // Tauri APIs not available in browser/dev; ignore
  }
}
