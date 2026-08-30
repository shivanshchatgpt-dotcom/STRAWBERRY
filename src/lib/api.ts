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
  createCalendarEvent: (params: {
    title: string;
    description?: string;
    startAt: string;
    endAt: string;
    timezone?: string;
    category?: string;
    sourceUrl?: string;
    location?: string;
    isAllDay?: boolean;
    certificateOffered?: boolean;
    registrationRequired?: boolean;
    reminderMinutes?: number[];
  }) => call<import("./types").CalendarEvent>("create_calendar_event", params),
  deleteCalendarEvent: (id: string) =>
    call<void>("delete_calendar_event", { id }),

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
    intervalMinutes: number,
  ) =>
    call<void>("wellness_set_category", {
      category,
      enabled,
      intervalMinutes,
    }),
  wellnessRecordActivity: (source: string) =>
    call<void>("wellness_record_activity", { source }),
  wellnessDismiss: () => call<void>("wellness_dismiss"),

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
    intervalMinutes: number;
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
