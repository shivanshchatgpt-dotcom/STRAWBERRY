import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  BreadcrumbItem,
  ChatDetail,
  HandoffExport,
  NodeSummary,
  Root,
  ScreenConfig,
  ScreenFrame,
  ScreenSearchHit,
  ScreenBlocklistItem,
  SearchScopeKind,
  SearchResult,
  UnifiedSearchItem,
  TreeNode,
} from "./types";

// ─────────────────────── Phase 3: Generic Memory API ────────────────────────

export type MemoryKind =
  | "working" | "episodic" | "semantic" | "project" | "procedural"
  | "credential" | "image" | "document" | "block" | "generic";

export type PrivacyLevel =
  | "public" | "normal" | "sensitive" | "private" | "secret";

export type RedactionState = "none" | "redacted" | "blocked";

export type RelationshipType =
  | "related_to" | "belongs_to" | "created_from" | "copied_from"
  | "derived_from" | "source_for" | "screenshot_of" | "captured_during"
  | "attached_to" | "references" | "part_of" | "produced_by"
  | "used_with" | "contains" | "parent_of" | "child_of"
  | "derived_relationship";

export interface Memory {
  id: string;
  kind: string;
  title: string;
  content: string;
  source: string;
  sourceRef: string | null;
  sourceApplication: string | null;
  sourceWindow: string | null;
  sourceWorkspace: string | null;
  sourceFile: string | null;
  sourceUrl: string | null;
  sourceSession: string | null;
  projectId: string | null;
  sessionId: string | null;
  category: string | null;
  tags: string[];
  confidence: number;
  sensitivity: number;
  privacyLevel: string;
  redactionState: string;
  appState: string;
  createdAtMs: number;
  updatedAtMs: number;
  occurredAtMs: number;
  firstSeenAtMs: number | null;
  lastSeenAtMs: number | null;
  lastViewedAtMs: number | null;
  lastCopiedAtMs: number | null;
  lastUsedAtMs: number | null;
  viewCount: number;
  copyCount: number;
  useCount: number;
  parentId: string | null;
  retentionDays: number | null;
  contentHash: string | null;
}

export interface SearchHit {
  memory: Memory;
  score: number;
  matchedVia: string[];
}

export interface SearchPage {
  hits: SearchHit[];
  total: number;
  limit: number;
  offset: number;
  hasMore: boolean;
}

export interface DocBlockLink {
  id: string;
  blockId: string;
  documentId: string;
  memoryId: string;
  blockType: string | null;
  createdAtMs: number;
}

export interface SecretStoreStatus {
  available: boolean;
  backend: string;
}

export interface Relationship {
  id: string;
  fromId: string;
  toId: string;
  relType: string;
  confidence: number;
  evidence: string | null;
  observed: boolean;
  createdAtMs: number;
}

// ──────────────────────── Credentials ────────────────────────

export interface CredentialMetadata {
  id: string;
  service: string;
  account: string | null;
  username: string | null;
  environment: string | null;
  host: string | null;
  project: string | null;
  url: string | null;
  notes: string | null;
  secretSet: boolean;
  lastUsedAtMs: number | null;
  createdAtMs: number;
  updatedAtMs: number;
}

// ──────────────────────── Images ────────────────────────

export type OcrStatus =
  | "pending" | "queued" | "running" | "done" | "failed"
  | "unavailable" | "skipped";

export interface ImageAsset {
  id: string;
  memoryId: string | null;
  originalPath: string;
  thumbnailPath: string | null;
  mimeType: string | null;
  width: number | null;
  height: number | null;
  byteSize: number | null;
  caption: string | null;
  sourceApp: string | null;
  sourceWindow: string | null;
  sourceProject: string | null;
  ocrText: string | null;
  ocrStatus: string;
  ocrCompletedAtMs: number | null;
  thumbnailStatus: string;
  thumbnailCompletedAtMs: number | null;
  privacyBlocked: boolean;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface OcrRunResult {
  imageId: string;
  status: OcrStatus;
  text: string | null;
  engine: string;
  error: string | null;
}

// ──────────────────────── Autonomy ────────────────────────

export interface RuntimeSnapshot {
  mode: "stopped" | "running" | "paused";
  stats: {
    mode: string;
    cyclesTotal: number;
    cyclesWithAction: number;
    eventsConsumedTotal: number;
    lastCycleAtMs: number;
    worldStateVersion: number;
    uptimeSecs: number;
  };
  worldState: {
    activeApp: string | null;
    activeProject: string | null;
    activeFile: { path: string; project: string | null; ts: number } | null;
    buildState: string;
    testState: string;
    workflowPhase: string;
    version: number;
  };
}

export interface LedgerEntry {
  id: number;
  capabilityId: string;
  decision: string;
  reason: string;
  score: number | null;
  createdAt: string;
}

export interface GoalCandidate {
  goalId: number;
  title: string;
  description: string;
  project: string | null;
  priority: string;
  confidence: number;
  evidence: { kind: string; reference: string; summary: string; weight: number }[];
  status: string;
  createdAt: string;
  expiresAt: string;
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
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

  getNodePath: (nodeId: string) =>
    call<string[]>("get_node_path", { nodeId }),

  // Tabs (Phase 30 integrity: backend commands were registered but unbound)
  recordTabVisit: (url: string, title?: string | null) =>
    call<void>("record_tab_visit", { url, title: title ?? null }),
  listTabGroups: (limit?: number) =>
    call<Array<{ host: string; url: string; title: string | null; visitedAt: string; visits: number }>>(
      "list_tab_groups",
      { limit },
    ),
  findTabsForTopic: (query: string, limit?: number) =>
    call<[string, string][]>("find_tabs_for_topic", { query, limit }),

  ping: () => call<string>("ping"),

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

  // Handoff -------------------------------------------------------------------
  /** Compress a saved chat into an AI-to-AI handoff packet. */
  exportHandoff: (chatId: string, tokenBudget: number | null) =>
    call<HandoffExport>("export_handoff", { chatId, tokenBudget }),

  /** Compress pasted text without saving it first. */
  handoffFromText: (
    title: string,
    text: string,
    tokenBudget: number | null,
  ) => call<HandoffExport>("handoff_from_text", { title, text, tokenBudget }),

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

  /** Unified search — chats, todos, habits, events, insights, alpha candidates. */
  searchAll: (query: string) =>
    call<UnifiedSearchItem[]>("search_all", { query }),

  /** Database overview — live counts + the 20 most recent Ctrl+C captures. */
  getDbOverview: () => call<DbOverviewData>("get_db_overview"),

  // 📄 DOCX workspace -------------------------------------------------------
  docxList: () => call<docx.DocxSummary[]>("docx_list"),
  docxNew: () => call<docx.DocxDocument>("docx_new"),
  docxOpen: (documentId: string) =>
    call<docx.DocxDocument>("docx_open", { documentId }),
  docxSave: (documentId: string, title: string, blocks: docx.Block[]) =>
    call<void>("docx_save", { args: { documentId, title, blocks } }),
  docxDelete: (documentId: string) =>
    call<void>("docx_delete", { documentId }),
  docxParsePaste: (input: docx.PasteInput) =>
    call<docx.Block[]>("docx_parse_paste", { input }),
  docxSearch: (query: string) =>
    call<docx.DocxSummary[]>("docx_search", { query }),
  docxExport: (documentId: string, format: "markdown" | "html" | "json") =>
    call<docx.DocxExport>("docx_export", { documentId, format }),

  // 🌳 Project Brain (Phase C/D/E) ------------------------------------------------

  getProjectBrain: () =>
    call<ProjectBrainData>("get_project_brain"),

  getWhatChanged: () =>
    call<WhatChangedData>("get_what_changed"),

  getIntelligentResume: () =>
    call<IntelligentResumeData>("get_intelligent_resume"),

  // Screen Memory -----------------------------------------------------------------
  startScreenCapture: (config?: ScreenConfig | null) =>
    call<void>("start_screen_capture", { config }),

  stopScreenCapture: () => call<void>("stop_screen_capture"),

  getScreenConfig: () => call<ScreenConfig>("get_screen_config"),

  updateScreenConfig: (config: ScreenConfig) =>
    call<void>("update_screen_config", { config }),

  listScreens: (
    limit?: number,
    offset?: number,
    appFilter?: string | null,
    fromTs?: number | null,
    toTs?: number | null,
  ) =>
    call<ScreenFrame[]>("list_screens", { limit, offset, appFilter, fromTs, toTs }),

  searchScreens: (query: string, limit?: number) =>
    call<ScreenSearchHit[]>("search_screens", { query, limit }),

  getScreenFrame: (id: number) =>
    call<ScreenFrame | null>("get_screen_frame", { id }),

  deleteScreenFrame: (id: number) => call<void>("delete_screen_frame", { id }),

  addScreenBlocklist: (pattern: string, reason?: string | null) =>
    call<number>("add_screen_blocklist", { pattern, reason }),

  removeScreenBlocklist: (id: number) => call<void>("remove_screen_blocklist", { id }),

  listScreenBlocklist: () => call<ScreenBlocklistItem[]>("list_screen_blocklist"),

  // Inbox ----------------------------------------------------------------------
  getInboxItems: (kind: string | null, limit?: number) =>
    call<inbox.InboxItem[]>("get_inbox_items", { kind, limit }),
  getInboxCounts: () => call<inbox.InboxCounts>("get_inbox_counts"),
  deleteInboxItem: (chatId: string) => call<void>("delete_inbox_item", { chatId }),

  // Story ---------------------------------------------------------------------
  exportMyStory: (repoPath: string | null, days?: number) =>
    call<string>("export_my_story", { repoPath, days }),

  // Health Lens ----------------------------------------------------------------
  healthReport: () => call<health.HealthReport>("health_report"),

  // Utilities -----------------------------------------------------------------
  getAppInfo: () => call<AppInfo>("get_app_info"),

  // Planner -------------------------------------------------------------------
  getTodos: () => call<planner.Todo[]>("get_todos"),
  addTodo: (title: string, priority: string, dueDate: string | null) =>
    call<planner.Todo>("add_todo", { title, priority, dueDate }),
  toggleTodo: (todoId: number) => call<boolean>("toggle_todo", { todoId }),
  deleteTodo: (todoId: number) => call<void>("delete_todo", { todoId }),

  getHabits: () => call<planner.Habit[]>("get_habits"),
  addHabit: (name: string, icon: string | null, targetDays: number | null) =>
    call<planner.Habit>("add_habit", { name, icon, targetDays }),
  toggleHabitToday: (habitId: number) =>
    call<boolean>("toggle_habit_today", { habitId }),

  getSchedule: () => call<planner.ScheduleEvent[]>("get_schedule"),
  addEvent: (title: string, startTime: string, endTime: string | null) =>
    call<planner.ScheduleEvent>("add_event", { title, startTime, endTime }),

  getDailyBriefing: () => call<planner.BriefingSection[]>("get_daily_briefing"),

  // Calendar Events -----------------------------------------------------------
  listCalendarEvents: (startRange?: string, endRange?: string) =>
    call<import("./types").CalendarEvent[]>("list_calendar_events", {
      startRange: startRange ?? null,
      endRange: endRange ?? null,
    }),
  createCalendarEvent: (params: import("./types").CalendarEventInput) =>
    call<import("./types").CalendarEvent>("create_calendar_event", {
      title: params.title,
      description: params.description ?? null,
      startAt: params.startAt,
      endAt: params.endAt,
      timezone: params.timezone ?? null,
      category: params.category ?? null,
      sourceUrl: params.sourceUrl ?? null,
      location: params.location ?? null,
      isAllDay: params.isAllDay ?? false,
      certificateOffered: params.certificateOffered ?? false,
      registrationRequired: params.registrationRequired ?? false,
      recurrence: params.recurrence ?? "none",
      recurrenceEnd: params.recurrenceEnd ?? null,
      color: params.color ?? null,
      reminderMinutes: params.reminderMinutes ?? null,
    }),
  updateCalendarEvent: (id: string, params: import("./types").CalendarEventInput) =>
    call<import("./types").CalendarEvent>("update_calendar_event", { id, ...params }),
  deleteCalendarEvent: (id: string) =>
    call<void>("delete_calendar_event", { id }),
  listEventReminders: (eventId: string) =>
    call<import("./types").EventReminder[]>("list_event_reminders", { eventId }),
  searchCalendarEvents: (query: string) =>
    call<import("./types").CalendarEvent[]>("search_calendar_events", { query }),

  // Resume ----------------------------------------------------------------------
  getResumeSuggestions: (limit = 5) =>
    call<resume.ResumePoint[]>("get_resume_suggestions", { limit }),
  saveResumePoint: (chatId: string) =>
    call<resume.ResumePoint>("save_resume_point", { chatId }),
  dismissResumePoint: (resumeId: string) =>
    call<void>("dismiss_resume_point", { resumeId }),
  getDaySummary: () => call<resume.DaySummary>("get_day_summary"),

  // Planner round 2 (focus + backfill) -------------------------------------------
  logFocusSession: (minutes: number, label?: string | null, kind?: "timer" | "stopwatch") =>
    call<planner.FocusSession>("log_focus_session", { minutes, label: label ?? null, kind: kind ?? null }),
  getFocusStats: () => call<planner.FocusStats>("get_focus_stats"),
  toggleHabitDate: (habitId: number, date: string) =>
    call<boolean>("toggle_habit_date", { habitId, date }),

  // 🧊 Freeze & Resume ------------------------------------------------------------
  freezeWorkSpace: () => call<ws.WorkSpace>("freeze_work_space"),
  listWorkSpaces: (limit = 20) =>
    call<ws.WorkSpaceRow[]>("list_work_spaces", { limit }),
  restoreWorkSpace: (id: string) =>
    call<ws.RestoreReport>("restore_work_space", { id }),
  deleteWorkSpace: (id: string) => call<void>("delete_work_space", { id }),

  // Workspace Resume v0.1 IPC ----------------------------------------------------
  captureWorkspaceSnapshot: () =>
    call<import("./types").WorkspaceSession>("capture_workspace_snapshot"),
  freezeWorkspace: () =>
    call<import("./types").WorkspaceSession>("freeze_workspace"),
  listWorkspaceSessions: (limit?: number) =>
    call<import("./types").WorkspaceSession[]>("list_workspace_sessions", { limit: limit ?? null }),
  getWorkspaceSession: (id: string) =>
    call<import("./types").WorkspaceSession | null>("get_workspace_session", { id }),
  resumeWorkspaceSession: (id: string) =>
    call<import("./types").WorkspaceSession>("resume_workspace_session", { id }),
  retryWorkspaceItem: (itemId: string) =>
    call<import("./types").ActionResult>("retry_workspace_item", { itemId }),
  deleteWorkspaceSession: (id: string) =>
    call<void>("delete_workspace_session", { id }),
  openWorkspaceItem: (itemId: string, confirmed?: boolean) =>
    call<import("./types").ActionResult>("open_workspace_item", { itemId, confirmed: confirmed ?? false }),

  // Context Recall (work snapshots) ---------------------------------------------
  captureWorkSnapshot: () => call<snap.WorkSnapshot>("capture_work_snapshot"),
  getLatestWorkSnapshot: () =>
    call<snap.WorkSnapshot | null>("get_latest_work_snapshot"),
  listWorkSnapshots: (limit = 10) =>
    call<[string, string, string | null][]>("list_work_snapshots", { limit }),

  // Ambient Memory & AST IPC
  recordAmbientEvent: (
    eventType: string,
    title: string,
    summary: string,
    sourceApp?: string,
    metadata?: string,
  ) =>
    invoke<import("./types").AmbientEvent>("record_ambient_event", {
      eventType,
      title,
      summary,
      sourceApp,
      metadata,
    }),
  getAmbientEvents: (limit?: number) =>
    invoke<import("./types").AmbientEvent[]>("get_ambient_events", { limit }),
  analyzeCodeAst: (langOrExt: string, source: string) =>
    invoke<import("./types").SymbolicAnalysis>("analyze_code_ast", {
      langOrExt,
      source,
    }),
  getAmbientStats: () =>
    invoke<import("./types").AmbientStats>("get_ambient_stats"),
  generateDeterministicReport: () =>
    invoke<import("./types").DeterministicReport>(
      "generate_deterministic_report",
    ),

  // Wellness --------------------------------------------------------------------
  wellnessGetState: () => call<wellness.WellnessState>("wellness_get_state"),
  wellnessSetEnabled: (enabled: boolean) =>
    call<void>("wellness_set_enabled", { enabled }),
  wellnessSnooze: (minutes: number) =>
    call<void>("wellness_snooze", { minutes }),
  wellnessGetConfig: () => call<wellness.WellnessConfig[]>("wellness_get_config"),
  wellnessSetCategory: (
    category: string,
    enabled: boolean,
    intervalSeconds: number,
  ) =>
    call<void>("wellness_set_category", {
      category,
      enabled,
      intervalSeconds,
    }),
  wellnessRecordActivity: (source: string) =>
    call<void>("wellness_record_activity", { source }),
  wellnessDismiss: () => call<void>("wellness_dismiss"),

  /** Fire a wellness popup right now — for testing the reminder pipeline. */
  wellnessTestPopup: (category?: string) =>
    call<void>("wellness_test_popup", { category: category ?? null }),

  // Alpha Hunter ---------------------------------------------------------------
  scanAlpha: () => call<alpha.ScanReport>("scan_alpha"),
  listAlphaCandidates: () => call<alpha.AlphaCandidate[]>("list_alpha_candidates"),
  verifyAlphaCandidate: (id: string, apiKey: string) =>
    call<alpha.AlphaCandidate>("verify_alpha_candidate", { id, apiKey }),
  dismissAlphaCandidate: (id: string) =>
    call<void>("dismiss_alpha_candidate", { id }),
  getAlphaConfig: (id: string) => call<string>("get_alpha_config", { id }),
  getAlphaEnabled: () => call<boolean>("get_alpha_enabled"),
  setAlphaEnabled: (enabled: boolean) =>
    call<boolean>("set_alpha_enabled", { enabled }),

  // Ghost ----------------------------------------------------------------------
  ghostRecordEvent: (
    eventType: string,
    sourceId: string,
    sourceKind: string,
    durationMs: number,
    metadata?: string | null,
  ) =>
    call<void>("ghost_record_event", {
      eventType,
      sourceId,
      sourceKind,
      durationMs,
      metadata: metadata ?? null,
    }),
  ghostRebuildGraph: () => call<void>("ghost_rebuild_graph"),
  ghostRegenerateInsights: () => call<void>("ghost_regenerate_insights"),
  ghostGetSnapshot: () => call<ghost.GhostSnapshot>("ghost_get_snapshot"),
  ghostMarkSeen: (insightId: number) =>
    call<void>("ghost_mark_seen", { insightId }),
  ghostPurge: () => call<void>("ghost_purge"),

  // Autonomy -------------------------------------------------------------------
  autonomyGetState: () => call<autonomy.RuntimeSnapshot>("autonomy_get_state"),
  autonomyStart: () => call<void>("autonomy_start"),
  autonomyPause: () => call<void>("autonomy_pause"),
  autonomyResume: () => call<void>("autonomy_resume"),
  autonomyShutdown: () => call<void>("autonomy_shutdown"),
  autonomyRunCycle: (batchSize: number) =>
    call<void>("autonomy_run_cycle", { batchSize }),
  autonomyPublish: (kind: string, data: Record<string, unknown>) =>
    call<void>("autonomy_publish", { kind, data }),

  // 🧩 Capability Registry + Scheduler ledger (Phase 6) --------------------
  listCapabilities: () =>
    call<autonomy.CapabilityState[]>("list_capabilities"),
  setCapabilityEnabled: (capabilityId: string, enabled: boolean) =>
    call<void>("set_capability_enabled", { capabilityId, enabled }),
  setCapabilityInterval: (capabilityId: string, intervalSecs: number) =>
    call<void>("set_capability_interval", { capabilityId, intervalSecs }),
  getCapabilityLedger: (limit?: number) =>
    call<autonomy.LedgerEntry[]>("get_capability_ledger", { limit }),

  // 🎯 Goal Engine (Phase 7) — deterministic candidates --------------------
  getGoalCandidates: () =>
    call<autonomy.GoalCandidate[]>("get_goal_candidates"),

  // 🗺️ Planner (Phase 8) — deterministic, non-executing plans ---------------
  getPlans: () =>
    call<Array<autonomy.PlannedResult>>("get_plans"),

  // ── AI / Intelligence ────────────────────────────────────────────────────
  aiGetStatus: () => call<Record<string, unknown>>("ai_get_status"),
  aiSetEnabled: (enabled: boolean) =>
    call<void>("ai_set_enabled", { enabled }),
  aiConfigureProvider: (args: {
    provider: string;
    model?: string;
    baseUrl?: string;
    apiKey?: string;
    name?: string;
  }) => call<void>("ai_configure_provider", args),
  aiTestConnection: (provider: string) =>
    call<string>("ai_test_connection", { provider }),
  aiListModels: (provider: string) =>
    call<string[]>("ai_list_models", { provider }),
  aiRemoveCredential: (provider: string) =>
    call<void>("ai_remove_credential", { provider }),

  // ─────────────────────── Phase 3: Generic Memory ────────────────────────
  memoryCreate: (args: {
    kind: string;
    title: string;
    content: string;
    source: string;
    project?: string;
    tags?: string[];
    sourceApplication?: string;
    sourceUrl?: string;
    sourceFile?: string;
  }) => call<string>("memory_create", { args }),

  memoryGet: (id: string) => call<Memory | null>("memory_get", { id }),

  memoryDelete: (id: string) => call<boolean>("memory_delete", { id }),

  memoryUpdate: (args: {
    id: string;
    title?: string;
    content?: string;
    kind?: string;
    tags?: string[];
    category?: string | null;
    project?: string | null;
    session?: string | null;
    sourceApplication?: string | null;
    sourceWindow?: string | null;
    sourceWorkspace?: string | null;
    sourceFile?: string | null;
    sourceUrl?: string | null;
    sourceSession?: string | null;
    privacyLevel?: string;
    sensitivity?: number;
    redactionState?: string;
    confidence?: number;
    retentionDays?: number | null;
    parentId?: string | null;
    occurredAtMs?: number;
  }) => call<Memory>("memory_update", { args }),

  memoryRecordView: (id: string) =>
    call<boolean>("memory_record_view", { id }),

  memoryRecordCopy: (id: string) =>
    call<boolean>("memory_record_copy", { id }),

  memoryRecordUse: (id: string) =>
    call<boolean>("memory_record_use", { id }),

  memorySearch: (args: {
    text: string;
    kind?: string;
    project?: string;
    app?: string;
    limit?: number;
    offset?: number;
  }) => call<SearchPage>("memory_search", { args }),

  memoryCreateRelationship: (args: {
    fromId: string;
    toId: string;
    relType: string;
    confidence?: number;
    evidence?: string;
  }) => call<string>("memory_create_relationship", { args }),

  memoryListRelationships: (id: string) =>
    call<Relationship[]>("memory_list_relationships", { id }),

  // ─────────────────────── Credentials ────────────────────────
  credentialCreate: (args: {
    title: string;
    service: string;
    account?: string;
    username?: string;
    environment?: string;
    host?: string;
    project?: string;
    url?: string;
    notes?: string;
    /** Opaque ciphertext bytes (encrypted by the UI layer). */
    secretCiphertext?: number[];
    secretNonce?: number[];
  }) => call<string>("credential_create", { args }),

  credentialGetMetadata: (id: string) =>
    call<CredentialMetadata | null>("credential_get_metadata", { id }),

  credentialSearch: (query: string, limit?: number) =>
    call<CredentialMetadata[]>("credential_search", { query, limit: limit ?? null }),

  /** The ONLY way to read a secret — must be an explicit user action. */
  credentialReveal: (id: string) =>
    call<number[] | null>("credential_reveal", { id }),

  credentialUpdateSecret: (id: string, ciphertext: number[], nonce: number[]) =>
    call<boolean>("credential_update_secret", {
      id, secretCiphertext: ciphertext, secretNonce: nonce,
    }),

  credentialDelete: (id: string) =>
    call<boolean>("credential_delete", { id }),

  // ─────────────────────── Images / OCR ────────────────────────
  imageRegister: (args: {
    path: string;
    mimeType?: string;
    width?: number;
    height?: number;
    byteSize?: number;
    caption?: string;
    sourceApp?: string;
    sourceWindow?: string;
    sourceProject?: string;
    privacyBlocked: boolean;
  }) => call<string>("image_register", { args }),

  imageGet: (id: string) => call<ImageAsset | null>("image_get", { id }),

  imageList: (limit?: number) =>
    call<ImageAsset[]>("image_list", { limit: limit ?? null }),

  imageDelete: (id: string) => call<boolean>("image_delete", { id }),

  imageSetOcrText: (id: string, text: string) =>
    call<boolean>("image_set_ocr_text", { id, text }),

  imageMarkOcrFailed: (id: string) =>
    call<boolean>("image_mark_ocr_failed", { id }),

  imageMarkOcrUnavailable: (id: string) =>
    call<boolean>("image_mark_ocr_unavailable", { id }),

  /** Run OCR on the next pending image. Returns null if no work. */
  imageOcrRunNext: () => call<OcrRunResult | null>("image_ocr_run_next"),

  // ─────────────────────── Watchers ────────────────────────
  watcherStart: (path: string) =>
    call<string>("watcher_start", { path }),

  watcherStop: (path: string) =>
    call<string>("watcher_stop", { path }),

  watcherList: () => call<string[]>("watcher_list"),

  watcherCheckPath: (path: string) =>
    call<boolean>("watcher_check_path", { path }),

  // ─────────────────────── Autonomy observability ────────────────────────
  autonomyGetStats: () => call<RuntimeSnapshot>("autonomy_get_stats"),

  autonomyGetLedger: (limit?: number) =>
    call<LedgerEntry[]>("autonomy_get_ledger", { limit: limit ?? null }),

  autonomyGetGoals: (limit?: number) =>
    call<GoalCandidate[]>("autonomy_get_goals", { limit: limit ?? null }),

  // ─────────────────────── DB overview (extend with memory counts) ─────
  /** Get a count of memories in the unified_memories table. */
  memoryCount: () => call<number>("memory_count"),

  // ─────────────────────── DOCX ↔ Memory linking ───────────────────────
  docxLinkBlockToMemory: (args: {
    blockId: string;
    documentId: string;
    blockType: string | null;
    memoryId: string;
  }) => call<DocBlockLink>("docx_link_block_to_memory", { args }),

  docxUnlinkBlockMemory: (blockId: string, memoryId: string) =>
    call<boolean>("docx_unlink_block_memory", { blockId, memoryId }),

  docxListBlockMemories: (blockId: string) =>
    call<DocBlockLink[]>("docx_list_block_memories", { blockId }),

  docxListMemoryBlocks: (memoryId: string) =>
    call<DocBlockLink[]>("docx_list_memory_blocks", { memoryId }),

  // ─────────────────────── Secret store status ───────────────────────
  credentialSecretStoreStatus: () =>
    call<SecretStoreStatus>("credential_secret_store_status"),
};

export namespace wellness {
  export interface WellnessState {
    enabled: boolean;
    nextReminderInSecs: number;
    lastCategory: string | null;
    snoozedUntil: string | null;
  }
  export interface WellnessConfig {
    category: string;
    enabled: boolean;
    /** Repeat interval in **seconds**. Convert to minutes/hours for display. */
    intervalSeconds: number;
    lastRemindedAt: string | null;
  }
}

export namespace alpha {
  export interface AlphaCandidate {
    id: string;
    source: string;
    title: string;
    url: string | null;
    provider: string | null;
    modelId: string | null;
    baseUrl: string | null;
    status: string;
    score: number;
    detectedAt: string;
    verifiedAt: string | null;
    notes: string | null;
  }
  export interface ScanReport {
    scannedSources: number;
    itemsChecked: number;
    newCandidates: number;
    enabled: boolean;
  }
}

export namespace ghost {
  export interface GraphNode {
    id: string;
    kind: "chat" | "folder" | "root" | "tag";
    label: string;
    weight: number;
    color: string | null;
    positionX: number | null;
    positionY: number | null;
  }
  export interface GraphEdge {
    id: number;
    sourceId: string;
    targetId: string;
    weight: number;
    edgeType: string;
  }
  export interface Graph {
    nodes: GraphNode[];
    edges: GraphEdge[];
  }
  export interface GhostInsight {
    id: number;
    kind: string;
    title: string;
    body: string;
    sourceIds: string | null;
    score: number;
    seen: number;
    createdAt: string;
  }
  export interface GhostEvent {
    id: number;
    eventType: string;
    sourceId: string | null;
    sourceKind: string | null;
    durationMs: number;
    metadata: string | null;
    createdAt: string;
  }
  export interface AttentionCell {
    day: number;
    hour: number;
    count: number;
    durationMs: number;
  }
  export interface GhostStats {
    totalEvents: number;
    totalChats: number;
    totalFolders: number;
    totalInsights: number;
    graphNodes: number;
    graphEdges: number;
    mostVisited: [string, string, number][];
    topTags: [string, number][];
    streakDays: number;
    peakHour: number | null;
    peakDay: number | null;
  }
  export interface GhostSnapshot {
    stats: GhostStats;
    graph: Graph;
    insights: GhostInsight[];
    heatmap: AttentionCell[];
    recentEvents: GhostEvent[];
  }
}

export namespace autonomy {
  export type RuntimeMode = "stopped" | "running" | "paused";
  export type BuildState =
    | "unknown"
    | "idle"
    | "running"
    | "succeeded"
    | { failed: { message: string } };
  export type TestState =
    | "unknown"
    | "idle"
    | "running"
    | { passed: { count: number } }
    | { failed: { passed: number; failed: number; message: string | null } };
  export interface RuntimeStats {
    mode: RuntimeMode;
    cyclesTotal: number;
    cyclesWithAction: number;
    eventsConsumedTotal: number;
    lastCycleAtMs: number;
    worldStateVersion: number;
    uptimeSecs: number;
  }

  // ── Capability Registry (Phase 6) ────────────────────────────────────
  export type CapabilityTrigger =
    | "event"
    | "debounced"
    | "interval"
    | "idle"
    | "session"
    | "daily"
    | "weekly";

  export type CapabilityRisk = "low" | "medium" | "high" | "forbidden";

  export type CapabilityLayer =
    | "instant"
    | "fast_brain"
    | "ghost"
    | "deep_background"
    | "daily"
    | "long_term";

  export interface CapabilityState {
    id: string;
    name: string;
    trigger: CapabilityTrigger;
    defaultIntervalSecs: number;
    layer: CapabilityLayer;
    risk: CapabilityRisk;
    resourceCost: number;
    privacySensitivity: number;
    valueWeight: number;
    dependsOn: string[];
    goal: string;
    enabled: boolean;
    intervalSecs: number;
    overrideReason: string | null;
  }

  export interface LedgerEntry {
    id: number;
    capabilityId: string;
    decision: string;
    reason: string;
    score: number | null;
    createdAt: string;
  }

  // ── Goal Engine (Phase 7) ────────────────────────────────────────────
  export interface GoalEvidence {
    kind: "task" | "error" | "resume" | "project";
    ref: string;
    summary: string;
    weight: number;
  }

  export type GoalStatus =
    | "candidate"
    | "accepted"
    | "completed"
    | "cancelled"
    | "expired";

  export type GoalPriority = "low" | "medium" | "high";

  export interface GoalCandidate {
    goalId: number;
    title: string;
    description: string;
    project: string | null;
    priority: GoalPriority;
    confidence: number;
    evidence: GoalEvidence[];
    status: GoalStatus;
    createdAt: string;
    expiresAt: string;
  }

  // ── Planner (Phase 8) ────────────────────────────────────────────────
  export type StepAction = "inspect" | "prepare" | "requires_approval";

  export interface PlanStep {
    stepId: number;
    capability: string;
    action: StepAction;
    purpose: string;
    targets: string[];
    prerequisites: number[];
    expectedResult: string;
    resourceCost: number;
    riskHint: string | null;
    order: number;
  }

  export interface Plan {
    planId: number;
    goalId: number;
    title: string;
    description: string;
    steps: PlanStep[];
    dependencies: [number, number][];
    estimatedCost: number;
    expectedOutcome: string;
    alternatives: PlanStep[];
    confidence: number;
    status: "draft" | "ready" | "rejected" | "stale";
    createdAt: string;
  }

  export interface PlanRejection {
    goalId: number;
    reason: string;
    createdAt: string;
  }

  /** One planning outcome: a real plan or a rejection reason. */
  export type PlannedResult =
    | { kind: "plan"; value: Plan }
    | { kind: "rejected"; value: PlanRejection };
  export interface WorldState {
    version: number;
    updatedAtMs: number;
    activeApp: string | null;
    activeWindowTitle: string | null;
    activeProject: string | null;
    activeFile: { path: string; project: string | null; openedAtMs: number } | null;
    workflowPhase: string;
    buildState: BuildState;
    testState: TestState;
    activeTasks: Array<{ id: number; title: string; status: string }>;
    recentFiles: Array<{ path: string; project: string | null; openedAtMs: number }>;
    recentCommands: Array<{ cmd: string; cwd: string | null; atMs: number }>;
    recentErrors: ErrorState[];
    recentAppSwitches: string[];
    recentSearches: string[];
    resource: {
      cpuPct: number;
      memUsedMb: number;
      memTotalMb: number;
      atMs: number;
    };
    lastActionAtMs: number;
    lastActionId: number | null;
    lastGoalId: number | null;
  }
  export interface ErrorState {
    message: string;
    source: string | null;
    atMs: number;
  }
  export interface RuntimeSnapshot {
    mode: RuntimeMode;
    stats: RuntimeStats;
    worldState: WorldState;
  }
}

export namespace health {
  export interface CacheSize {
    path: string;
    bytes: number;
  }
  export interface HealthReport {
    supported: boolean;
    diskFreeBytes: number;
    diskTotalBytes: number;
    caches: CacheSize[];
    topHomeDirs: CacheSize[];
    notes: string[];
  }
}

/** 🗄️ Database overview payload. */
export interface DbOverviewData {
  overview: {
    roots: number;
    folders: number;
    chats: number;
    captures: number;
    captureNotes: number;
    captureCode: number;
    captureErrors: number;
    captureUrls: number;
    todosOpen: number;
    todosDone: number;
    habits: number;
    events: number;
    alphaCandidates: number;
    insights: number;
    dbSizeBytes: number;
  };
  recent: Array<{
    chatId: string;
    title: string;
    kind: string;
    createdAt: string;
  }>;
}

/** 🌳 Project Brain (Phase C). */
export interface ProjectBrainData {
  projects: Array<{
    path: string;
    name: string;
    origin: string;
    lastSeenAt: number;
    openTasks: string[];
    tasksDone: number;
    recentErrors: string[];
    decisions: string[];
    activity: [string, number][];
    nextLikelyAction: string;
  }>;
}

/** 🔄 What Changed (Phase E). */
export interface WhatChangedData {
  baselineNote: string;
  since: string;
  tasksCompleted: string[];
  tasksAdded: string[];
  newCaptures: string[];
  newChats: string[];
  habitsDone: string[];
  newEvents: string[];
  activity: [string, number][];
  summary: string;
}

/** ⏮️ Intelligent Resume (Phase D). */
export interface IntelligentResumeData {
  narrative: {
    headline: string;
    since: string;
    changedSummary: string;
    plan: string[];
    focusProject: string | null;
  };
  changes: WhatChangedData;
}

export namespace inbox {
  export interface InboxItem {
    chatId: string;
    title: string;
    kind: string | null;
    preview: string | null;
    createdAt: string;
  }
  export interface InboxCounts {
    all: number;
    note: number;
    code: number;
    error: number;
    url: number;
    image?: number;
    diagram?: number;
  }
}

export namespace resume {
  export interface DaySummary {
    lastChats: [string, string][];
    lastCaptures: [string, string][];
    openTasks: string[];
    topIntent: string | null;
  }
  export interface ResumePoint {
    id: string;
    chatId: string | null;
    chatTitle: string | null;
    intent: string;
    lastExchange: string | null;
    openItems: string[];
    contextRefs: string[];
    updatedAt: string;
  }
}

export namespace planner {
  export interface Todo {
    id: number;
    title: string;
    description: string | null;
    priority: "low" | "medium" | "high";
    completed: boolean;
    dueDate: string | null;
  }
  export interface Habit {
    id: number;
    name: string;
    description: string | null;
    color: string | null;
    icon: string | null;
    targetDays: number;
    completedDates: string[];
  }
  export interface ScheduleEvent {
    id: number;
    title: string;
    description: string | null;
    startTime: string;
    endTime: string | null;
    color: string | null;
    recurring: string;
    completed: boolean;
  }
  export interface BriefingSection {
    key: string;
    title: string;
    lines: string[];
  }
  export interface FocusSession {
    id: number;
    minutes: number;
    label: string | null;
    kind: "timer" | "stopwatch";
    completedAt: string;
  }
  export interface FocusStats {
    sessions: number;
    totalMinutes: number;
    todayMinutes: number;
    todaySessions: number;
    recent: FocusSession[];
  }
}

export namespace snap {
  export interface WindowInfo {
    app: string;
    title: string;
    active: boolean;
  }
  export interface TabInfo {
    title: string;
    url: string;
  }
  export interface BrowserContext {
    browser: string;
    kind: "tabs" | "history";
    items: TabInfo[];
  }
  export interface RelatedNote {
    chatId: string;
    title: string;
  }
  export interface WorkSnapshot {
    id: string;
    createdAt: string;
    windows: WindowInfo[];
    browsers: BrowserContext[];
    clipboardHint: string | null;
    relatedNotes: RelatedNote[];
    story: string;
  }
}


export namespace ws {
  export interface FrozenWindow {
    app: string;
    title: string;
    x: number;
    y: number;
    w: number;
    h: number;
    desktop: number;
    active: boolean;
    launch?: string | null;
  }
  export interface BrowserRestore {
    browser: string;
    urls: string[];
  }
  export interface DevServer {
    port: number;
    pid?: number | null;
    procName: string;
    cwd: string;
    cmd: string;
  }
  export interface WorkSpace {
    id: string;
    name: string;
    story: string;
    createdAt: string;
    windows: FrozenWindow[];
    browsers: BrowserRestore[];
    terminals: string[];
    devServers: DevServer[];
  }
  export interface RestoreReport {
    launched: string[];
    failed: string[];
    pendingServers: DevServer[];
  }
  /** id, name, story, createdAt */
  export type WorkSpaceRow = [string, string, string, string];
}

// ---------------------------------------------------------------------------
// 📄 DOCX — offline block-document workspace
// ---------------------------------------------------------------------------

export namespace docx {
  export type BlockType =
    | "text" | "heading" | "table" | "formula" | "tree" | "graph"
    | "chart" | "todo" | "image" | "code" | "divider" | "callout";

  export interface TableProps {
    headerRow: boolean;
    borderThickness: number;
    borderColor: string;
    zebra: boolean;
    cellPadding: number;
    align: string;
    outerBorder: boolean;
  }

  export interface ChartConfig {
    chartType: "bar" | "line" | "pie" | "scatter";
    title: string;
    xLabel: string;
    yLabel: string;
    showLegend: boolean;
    sourceBlockId: string | null;
    annotations: Array<{ id: string; text: string; x: number; y: number; fontSize: number }>;
    data: string[][];
  }

  export interface TodoTask {
    id: string;
    text: string;
    done: boolean;
    priority: number;
  }

  export interface TreeNode {
    id: string;
    text: string;
    children: TreeNode[];
    collapsed: boolean;
  }

  /** One typed block. `data` carries the per-type payload. */
  export interface Block {
    id: string;
    type: BlockType;
    data: Record<string, unknown>;
  }

  export interface DocxDocument {
    id: string;
    title: string;
    blocks: Block[];
    createdAt: string;
    updatedAt: string;
  }

  export interface DocxSummary {
    id: string;
    title: string;
    preview: string;
    updatedAt: string;
  }

  export interface DocxExport {
    filename: string;
    content: string;
  }

  export interface PasteInput {
    html: string | null;
    text: string | null;
  }
}
