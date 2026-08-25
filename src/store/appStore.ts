import { create } from "zustand";
import { api } from "../lib/api";
import type {
  BreadcrumbItem,
  ChatDetail,
  HandoffExport,
  NodeSummary,
  Root,
  SearchScopeKind,
  SearchResult,
} from "../lib/types";

// ---------------------------------------------------------------------------
// View / dialog models
// ---------------------------------------------------------------------------

export interface DeleteTarget {
  type: "root" | "folder" | "chat";
  id: string;
  /** For chats: the underlying chat id used by the API. */
  chatId?: string;
  name: string;
}

export type DialogState =
  | { kind: "none" }
  | { kind: "create-root" }
  | { kind: "create-folder"; parentId: string | null }
  | { kind: "create-chat"; parentId: string | null }
  | { kind: "import-chat"; parentId: string | null }
  | { kind: "rename-root"; rootId: string; name: string }
  | { kind: "rename-folder"; nodeId: string; name: string }
  | { kind: "confirm-delete"; target: DeleteTarget };

export interface Toast {
  id: number;
  severity: "success" | "error" | "info";
  message: string;
}

export interface SearchScope {
  kind: SearchScopeKind;
  id: string | null;
  label: string;
}

function cacheKey(rootId: string, parentId: string | null): string {
  return `${rootId}::${parentId ?? ""}`;
}

let toastSeq = 1;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

interface AppState {
  // data
  roots: Root[];
  rootsLoading: boolean;
  currentRootId: string | null;
  currentNodeId: string | null;
  currentChatId: string | null;
  breadcrumb: BreadcrumbItem[];
  expandedNodeIds: Set<string>;
  childrenCache: Record<string, NodeSummary[]>;
  chatDetail: ChatDetail | null;
  appError: string | null;

  /** Handoff packet for the open chat; null until built. */
  handoff: HandoffExport | null;
  handoffBudget: number;
  handoffLoading: boolean;

  // search
  searchQuery: string;
  searchScope: SearchScope | null;
  searchResults: SearchResult[] | null;
  searching: boolean;

  // ui
  dialog: DialogState;
  toasts: Toast[];
  busy: boolean;

  // actions
  loadRoots: () => Promise<void>;
  openRoot: (rootId: string) => Promise<void>;
  openFolder: (nodeId: string) => Promise<void>;
  openChat: (chatId: string) => Promise<void>;
  goHome: () => void;
  navigateBreadcrumb: (item: BreadcrumbItem | null) => Promise<void>;
  toggleNode: (nodeId: string) => Promise<void>;
  ensureChildren: (rootId: string, parentId: string | null) => Promise<void>;

  createRoot: (name: string) => Promise<boolean>;
  renameRoot: (rootId: string, name: string) => Promise<boolean>;
  deleteRoot: (target: DeleteTarget) => Promise<boolean>;

  createFolder: (parentId: string | null, name: string) => Promise<boolean>;
  renameFolder: (nodeId: string, name: string) => Promise<boolean>;
  deleteFolder: (target: DeleteTarget) => Promise<boolean>;

  createChat: (
    parentId: string | null,
    title: string,
    text: string,
    tags: string | null,
  ) => Promise<boolean>;
  importChat: (
    parentId: string | null,
    filename: string,
    text: string,
    tags: string | null,
  ) => Promise<boolean>;
  deleteChat: (target: DeleteTarget) => Promise<boolean>;
  renameChat: (chatId: string, title: string) => Promise<boolean>;
  regenerateBrief: () => Promise<void>;

  /** Build the handoff packet for the open chat at the current budget. */
  buildHandoff: () => Promise<void>;
  setHandoffBudget: (tokens: number) => void;
  /** Copy text to the clipboard and toast the outcome. */
  copyText: (text: string, what: string) => Promise<void>;

  setSearchQuery: (q: string) => void;
  setSearchScope: (scope: SearchScope) => void;
  runSearch: () => Promise<void>;
  clearSearch: () => void;

  openDialog: (d: DialogState) => void;
  closeDialog: () => void;
  showToast: (severity: Toast["severity"], message: string) => void;
  dismissToast: (id: number) => void;
}

export const useAppStore = create<AppState>((set, get) => {
  // --- internal helpers ----------------------------------------------------

  const fail = (e: unknown): false => {
    const message = typeof e === "string" ? e : String(e);
    set({ appError: message });
    get().showToast("error", message);
    return false;
  };

  async function fetchChildrenInto(
    rootId: string,
    parentId: string | null,
  ): Promise<NodeSummary[]> {
    try {
      const children = await api.getChildren(rootId, parentId);
      set((s) => ({
        childrenCache: {
          ...s.childrenCache,
          [cacheKey(rootId, parentId)]: children,
        },
      }));
      return children;
    } catch (e) {
      fail(e);
      return [];
    }
  }

  function invalidateRootCache(rootId: string) {
    set((s) => {
      const next: Record<string, NodeSummary[]> = {};
      for (const [k, v] of Object.entries(s.childrenCache)) {
        if (!k.startsWith(`${rootId}::`)) next[k] = v;
      }
      return { childrenCache: next };
    });
  }

  async function refreshVisibleList() {
    const s = get();
    if (!s.currentRootId) return;
    if (s.currentNodeId === null && !s.currentChatId) {
      await fetchChildrenInto(s.currentRootId, null);
    } else if (s.currentNodeId) {
      await fetchChildrenInto(s.currentRootId, s.currentNodeId);
    } else if (s.currentChatId && s.chatDetail) {
      const parent = s.chatDetail.meta.nodeId;
      const node = await findNode(parent);
      const parentId = node?.parentId ?? null;
      await fetchChildrenInto(s.chatDetail.meta.rootId, parentId);
    }
  }

  async function findNode(nodeId: string): Promise<NodeSummary | undefined> {
    for (const list of Object.values(get().childrenCache)) {
      const hit = list.find((n) => n.id === nodeId);
      if (hit) return hit;
    }
    return undefined;
  }

  return {
    roots: [],
    rootsLoading: true,
    currentRootId: null,
    currentNodeId: null,
    currentChatId: null,
    breadcrumb: [],
    expandedNodeIds: new Set<string>(),
    childrenCache: {},
    chatDetail: null,
    appError: null,

    handoff: null,
    // Matches strawberry_core::handoff::DEFAULT_TOKEN_BUDGET.
    handoffBudget: 700,
    handoffLoading: false,

    searchQuery: "",
    searchScope: null,
    searchResults: null,
    searching: false,

    dialog: { kind: "none" },
    toasts: [],
    busy: false,

    // ------------------------------------------------------------------ data

    loadRoots: async () => {
      set({ rootsLoading: true });
      try {
        const roots = await api.getRoots();
        set({ roots });
      } catch (e) {
        fail(e);
      } finally {
        set({ rootsLoading: false });
      }
    },

    openRoot: async (rootId) => {
      const root = get().roots.find((r) => r.id === rootId);
      if (!root) return;
      set({
        currentRootId: rootId,
        currentNodeId: null,
        currentChatId: null,
        chatDetail: null,
        handoff: null,
        breadcrumb: [{ id: root.id, label: root.name, kind: "root" }],
      });
      await fetchChildrenInto(rootId, null);
    },

    openFolder: async (nodeId) => {
      const s = get();
      if (!s.currentRootId) return;
      let node = await findNode(nodeId);
      if (!node) {
        // Possibly loaded under a different parent; fetch direct children of
        // breadcrumb tail as a fallback.
        const tail = s.breadcrumb[s.breadcrumb.length - 1];
        const siblings = await fetchChildrenInto(
          s.currentRootId,
          tail && tail.kind === "folder" ? tail.id : null,
        );
        node = siblings.find((n) => n.id === nodeId);
      }
      if (!node || node.type !== "folder") return;
      set({
        currentNodeId: nodeId,
        currentChatId: null,
        chatDetail: null,
        handoff: null,
        breadcrumb: [
          ...s.breadcrumb.filter((b) => b.id !== nodeId),
          { id: node.id, label: node.name, kind: "folder" },
        ],
      });
      await fetchChildrenInto(s.currentRootId, nodeId);
    },

    openChat: async (chatId) => {
      try {
        const detail = await api.getChat(chatId);
        const crumbs = await api.getBreadcrumb(detail.meta.nodeId);
        set({
          currentChatId: chatId,
          chatDetail: detail,
          // Stale packet from the previously open chat must never linger.
          handoff: null,
          currentNodeId: null,
          breadcrumb: crumbs,
          currentRootId: detail.meta.rootId,
        });
        // Preload sidebar children for the chat's parent so the tree shows it.
        const parentKey = cacheKey(detail.meta.rootId, parentOf(crumbs));
        if (!get().childrenCache[parentKey]) {
          await fetchChildrenInto(detail.meta.rootId, parentOf(crumbs));
        }
      } catch (e) {
        fail(e);
      }
    },

    goHome: () => {
      set({
        currentRootId: null,
        currentNodeId: null,
        currentChatId: null,
        chatDetail: null,
        handoff: null,
        breadcrumb: [],
        searchResults: null,
      });
    },

    navigateBreadcrumb: async (item) => {
      if (item === null) {
        get().goHome();
        return;
      }
      if (item.kind === "root") {
        await get().openRoot(item.id);
        return;
      }
      const s = get();
      const idx = s.breadcrumb.findIndex((b) => b.id === item.id);
      if (idx >= 0) {
        set({ breadcrumb: s.breadcrumb.slice(0, idx + 1), currentNodeId: item.id });
        if (s.currentRootId) await fetchChildrenInto(s.currentRootId, item.id);
      } else {
        await get().openFolder(item.id);
      }
    },

    ensureChildren: async (rootId, parentId) => {
      const key = cacheKey(rootId, parentId);
      if (!get().childrenCache[key]) {
        await fetchChildrenInto(rootId, parentId);
      }
    },

    toggleNode: async (nodeId) => {
      const s = get();
      const wasExpanded = s.expandedNodeIds.has(nodeId);
      const next = new Set(s.expandedNodeIds);
      if (wasExpanded) {
        next.delete(nodeId);
        set({ expandedNodeIds: next });
        return;
      }
      next.add(nodeId);
      set({ expandedNodeIds: next });
      if (s.currentRootId) {
        const key = cacheKey(s.currentRootId, nodeId);
        if (!s.childrenCache[key]) {
          await fetchChildrenInto(s.currentRootId, nodeId);
        }
      }
    },

    // ----------------------------------------------------------------- roots

    createRoot: async (name) => {
      set({ busy: true });
      try {
        await api.createRoot(name);
        await get().loadRoots();
        get().showToast("success", `Index "${name.trim()}" created`);
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    renameRoot: async (rootId, name) => {
      set({ busy: true });
      try {
        await api.renameRoot(rootId, name);
        await get().loadRoots();
        const s = get();
        if (s.currentRootId === rootId) {
          set({
            breadcrumb: s.breadcrumb.map((b) =>
              b.id === rootId ? { ...b, label: name.trim() } : b,
            ),
          });
        }
        get().showToast("success", "Index renamed");
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    deleteRoot: async (target) => {
      set({ busy: true });
      try {
        await api.deleteRoot(target.id);
        await get().loadRoots();
        if (get().currentRootId === target.id) get().goHome();
        get().showToast("success", `Index "${target.name}" deleted`);
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    // --------------------------------------------------------------- folders

    createFolder: async (parentId, name) => {
      const s = get();
      if (!s.currentRootId) return false;
      set({ busy: true });
      try {
        await api.createFolder(s.currentRootId, parentId, name);
        invalidateRootCache(s.currentRootId);
        await refreshVisibleList();
        get().showToast("success", `Folder "${name.trim()}" created`);
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    renameFolder: async (nodeId, name) => {
      set({ busy: true });
      try {
        await api.renameFolder(nodeId, name);
        const s = get();
        if (s.currentRootId) invalidateRootCache(s.currentRootId);
        await refreshVisibleList();
        set((st) => ({
          breadcrumb: st.breadcrumb.map((b) =>
            b.id === nodeId ? { ...b, label: name.trim() } : b,
          ),
        }));
        get().showToast("success", "Renamed");
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    deleteFolder: async (target) => {
      set({ busy: true });
      try {
        await api.deleteFolder(target.id);
        const s = get();
        if (s.currentRootId) invalidateRootCache(s.currentRootId);
        // If we are inside the deleted subtree, climb out to home/root.
        if (
          s.currentNodeId === target.id ||
          s.breadcrumb.some((b) => b.id === target.id)
        ) {
          if (s.currentRootId) await get().openRoot(s.currentRootId);
        } else {
          await refreshVisibleList();
        }
        get().showToast("success", `Folder "${target.name}" deleted`);
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    // ----------------------------------------------------------------- chats

    createChat: async (parentId, title, text, tags) => {
      const s = get();
      if (!s.currentRootId) return false;
      set({ busy: true });
      try {
        const detail = await api.createChatFromText(
          s.currentRootId,
          parentId,
          title,
          text,
          tags,
        );
        invalidateRootCache(s.currentRootId);
        await refreshVisibleList();
        get().showToast("success", `Chat "${detail.meta.title}" saved`);
        get().closeDialog();
        await get().openChat(detail.meta.chatId);
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    importChat: async (parentId, filename, text, tags) => {
      const s = get();
      if (!s.currentRootId) return false;
      set({ busy: true });
      try {
        const detail = await api.importChatFileText(
          s.currentRootId,
          parentId,
          filename,
          text,
          tags,
        );
        invalidateRootCache(s.currentRootId);
        await refreshVisibleList();
        get().showToast("success", `Imported "${detail.meta.title}"`);
        get().closeDialog();
        await get().openChat(detail.meta.chatId);
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    deleteChat: async (target) => {
      if (!target.chatId) return false;
      set({ busy: true });
      try {
        await api.deleteChat(target.chatId);
        const s = get();
        if (s.currentRootId) invalidateRootCache(s.currentRootId);
        if (s.currentChatId === target.chatId) {
          // Return to the containing folder (or root) after deletion.
          const nodeId = s.chatDetail?.meta.nodeId;
          set({ currentChatId: null, chatDetail: null, handoff: null });
          if (nodeId) {
            const node = await findNode(nodeId);
            if (node?.parentId != null) {
              await get().openFolder(node.parentId);
            } else if (s.currentRootId) {
              await get().openRoot(s.currentRootId);
            }
          } else if (s.currentRootId) {
            await get().openRoot(s.currentRootId);
          }
        } else {
          await refreshVisibleList();
        }
        get().showToast("success", `Chat "${target.name}" deleted`);
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    renameChat: async (chatId, title) => {
      set({ busy: true });
      try {
        const detail = await api.updateChatMetadata(chatId, title, null);
        set({ chatDetail: detail });
        const s = get();
        if (s.currentRootId) invalidateRootCache(s.currentRootId);
        await refreshVisibleList();
        get().showToast("success", "Chat renamed");
        get().closeDialog();
        return true;
      } catch (e) {
        return fail(e);
      } finally {
        set({ busy: false });
      }
    },

    regenerateBrief: async () => {
      const s = get();
      if (!s.currentChatId || !s.chatDetail) return;
      set({ busy: true });
      try {
        const detail = await api.regenerateBrief(s.currentChatId);
        // Artifacts changed, so any packet built from the old ones is stale.
        set({ chatDetail: detail, handoff: null });
        get().showToast("success", "Brief regenerated");
      } catch (e) {
        fail(e);
      } finally {
        set({ busy: false });
      }
    },

    // --------------------------------------------------------------- handoff

    buildHandoff: async () => {
      const s = get();
      if (!s.currentChatId) return;
      set({ handoffLoading: true });
      try {
        const result = await api.exportHandoff(
          s.currentChatId,
          s.handoffBudget,
        );
        set({ handoff: result });
      } catch (e) {
        fail(e);
      } finally {
        set({ handoffLoading: false });
      }
    },

    setHandoffBudget: (tokens) => {
      // Clamped to the same range the Rust command enforces.
      const clamped = Math.min(8000, Math.max(120, Math.round(tokens)));
      set({ handoffBudget: clamped, handoff: null });
    },

    copyText: async (text, what) => {
      try {
        await navigator.clipboard.writeText(text);
        get().showToast("success", `${what} copied to clipboard`);
      } catch {
        // Clipboard access can be refused; the textarea remains selectable.
        get().showToast(
          "error",
          `Could not copy ${what}. Select the text and copy manually.`,
        );
      }
    },

    // ---------------------------------------------------------------- search

    setSearchQuery: (q) => {
      set({ searchQuery: q });
      if (q.trim() === "") set({ searchResults: null });
    },

    setSearchScope: (scope) => {
      set({ searchScope: scope });
      if (get().searchQuery.trim()) void get().runSearch();
    },

    runSearch: async () => {
      const q = get().searchQuery.trim();
      if (!q) {
        set({ searchResults: null });
        return;
      }
      const s = get();
      const scope: SearchScope =
        s.searchScope ??
        (s.currentNodeId
          ? { kind: "folder", id: s.currentNodeId, label: "This folder" }
          : s.currentRootId
            ? { kind: "root", id: s.currentRootId, label: "This index" }
            : { kind: "global", id: null, label: "Global" });
      set({ searching: true });
      try {
        const results = await api.searchChats(q, scope.kind, scope.id);
        set({ searchResults: results });
      } catch (e) {
        fail(e);
      } finally {
        set({ searching: false });
      }
    },

    clearSearch: () => {
      set({ searchQuery: "", searchResults: null, searchScope: null });
    },

    // -------------------------------------------------------------------- ui

    openDialog: (d) => set({ dialog: d }),
    closeDialog: () => set({ dialog: { kind: "none" } }),

    showToast: (severity, message) => {
      const id = toastSeq++;
      set((s) => ({ toasts: [...s.toasts, { id, severity, message }] }));
      window.setTimeout(() => get().dismissToast(id), 4000);
    },

    dismissToast: (id) =>
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  };
});

/** Parent folder id implied by a breadcrumb trail ([root, ...folders]). */
function parentOf(crumbs: BreadcrumbItem[]): string | null {
  const last = crumbs[crumbs.length - 1];
  return last && last.kind === "folder" ? last.id : null;
}
