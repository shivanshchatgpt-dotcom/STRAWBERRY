import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  BreadcrumbItem,
  ChatDetail,
  NodeSummary,
  Root,
  SearchScopeKind,
  SearchResult,
  TreeNode,
} from "./types";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args ?? {});
  } catch (e) {
    throw typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  }
}

export const api = {
  // Roots ---------------------------------------------------------------------
  getRoots: () => call<Root[]>("get_roots"),

  createRoot: (name: string) =>
    call<Root>("create_root", { name, color: null, icon: null }),

  renameRoot: (rootId: string, name: string) =>
    call<Root>("rename_root", { rootId, name }),

  deleteRoot: (rootId: string) => call<void>("delete_root", { rootId }),

  // Tree / navigation ---------------------------------------------------------
  getChildren: (rootId: string, parentId: string | null) =>
    call<NodeSummary[]>("get_children", { rootId, parentId }),

  getRootTree: (rootId: string) =>
    call<TreeNode[]>("get_root_tree", { rootId }),

  getBreadcrumb: (nodeId: string) =>
    call<BreadcrumbItem[]>("get_breadcrumb", { nodeId }),

  // Folders -------------------------------------------------------------------
  createFolder: (rootId: string, parentId: string | null, name: string) =>
    call<NodeSummary>("create_folder", { rootId, parentId, name }),

  renameFolder: (nodeId: string, name: string) =>
    call<NodeSummary>("rename_folder", { nodeId, name }),

  deleteFolder: (nodeId: string) => call<void>("delete_folder", { nodeId }),

  moveFolder: (nodeId: string, newParentId: string | null) =>
    call<NodeSummary>("move_folder", { nodeId, newParentId }),

  moveChat: (chatId: string, newParentId: string | null) =>
    call<NodeSummary>("move_chat", { chatId, newParentId }),

  // Chats ---------------------------------------------------------------------
  createChatFromText: (
    rootId: string,
    parentId: string | null,
    title: string,
    text: string,
    tags: string | null,
  ) =>
    call<ChatDetail>("create_chat_from_text", {
      rootId,
      parentId,
      title,
      text,
      tags,
    }),

  importChatFileText: (
    rootId: string,
    parentId: string | null,
    filename: string,
    text: string,
    tags: string | null,
  ) =>
    call<ChatDetail>("import_chat_file_text", {
      rootId,
      parentId,
      filename,
      text,
      tags,
    }),

  getChat: (chatId: string) => call<ChatDetail>("get_chat", { chatId }),

  getChatRaw: (chatId: string) => call<string>("get_chat_raw", { chatId }),

  deleteChat: (chatId: string) => call<void>("delete_chat", { chatId }),

  updateChatMetadata: (
    chatId: string,
    title: string | null,
    tags: string | null,
  ) => call<ChatDetail>("update_chat_metadata", { chatId, title, tags }),

  regenerateBrief: (chatId: string) =>
    call<ChatDetail>("regenerate_brief", { chatId }),

  // Search --------------------------------------------------------------------
  searchChats: (
    query: string,
    scopeKind: SearchScopeKind,
    scopeId: string | null,
  ) =>
    call<SearchResult[]>("search_chats", {
      query,
      scopeKind,
      scopeId,
    }),

  // Utilities -----------------------------------------------------------------
  getAppInfo: () => call<AppInfo>("get_app_info"),
};
