import { useEffect } from "react";
import { useAppStore } from "./store/appStore";
import { AppLayout } from "./components/Layout/AppLayout";
import { HomeView } from "./components/Home/HomeView";
import { BrowserView } from "./components/Browser/BrowserView";
import { ChatReaderView } from "./components/ChatReader/ChatReaderView";
import { SearchResultsView } from "./components/Search/SearchResultsView";
import { DialogsHost } from "./components/Dialogs/DialogsHost";
import { ToastHost } from "./components/Toast/ToastHost";

export default function App() {
  const loadRoots = useAppStore((s) => s.loadRoots);
  const currentRootId = useAppStore((s) => s.currentRootId);
  const currentChatId = useAppStore((s) => s.currentChatId);
  const searchResults = useAppStore((s) => s.searchResults);
  const openDialog = useAppStore((s) => s.openDialog);
  const closeDialog = useAppStore((s) => s.closeDialog);
  const runSearch = useAppStore((s) => s.runSearch);
  const clearSearch = useAppStore((s) => s.clearSearch);

  useEffect(() => {
    void loadRoots();
  }, [loadRoots]);

  // Global keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        document.getElementById("global-search")?.focus();
      } else if (mod && e.shiftKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        openDialog({ kind: "create-root" });
      } else if (mod && !e.shiftKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        if (!currentRootId || currentChatId) {
          openDialog({ kind: "create-root" });
        } else {
          openDialog({ kind: "create-folder", parentId: null });
        }
      } else if (e.key === "Escape") {
        closeDialog();
        clearSearch();
      } else if (e.key === "Enter" && (e.target as HTMLElement)?.id === "global-search") {
        void runSearch();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openDialog, closeDialog, currentRootId, currentChatId, runSearch, clearSearch]);

  const showSearch = searchResults !== null;

  let content: JSX.Element;
  if (showSearch) {
    content = <SearchResultsView />;
  } else if (currentChatId) {
    content = <ChatReaderView />;
  } else if (currentRootId) {
    content = <BrowserView />;
  } else {
    content = <HomeView />;
  }

  return (
    <AppLayout>
      {content}
      <DialogsHost />
      <ToastHost />
    </AppLayout>
  );
}
