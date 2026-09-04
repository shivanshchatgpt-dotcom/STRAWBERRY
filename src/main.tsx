import React from "react";
import ReactDOM from "react-dom/client";
import App, { PopupApp } from "./App";
import "./styles/global.css";

// Theme bootstrap — light is the default. We migrated the storage key from
// "strawberry-theme-v2" to "...-v3" so the new bright UI lands correctly
// even if the user previously persisted "dark" in an older build.
const _v3 = localStorage.getItem("strawberry-theme-v3") as "dark" | "light" | null;
const _initial: "dark" | "light" = _v3 === "dark" || _v3 === "light" ? _v3 : "light";
document.documentElement.dataset.theme = _initial;
// best-effort: clean up the v2 key so it doesn't get reused
try { localStorage.removeItem("strawberry-theme-v2"); } catch { /* noop */ }

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
  // Async IIFE (not top-level await): the Vite build targets es2021, where
  // top-level await is unsupported. Behavior is unchanged.
  void (async () => {
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
  })();
}
