export interface Root {
  id: string;
  name: string;
  color: string | null;
  icon: string | null;
  createdAt: string;
  updatedAt: string;
}

export type NodeKind = "folder" | "chat";

export interface NodeSummary {
  id: string;
  rootId: string;
  parentId: string | null;
  type: NodeKind;
  name: string;
  position: number;
  createdAt: string;
  updatedAt: string;
  /** Present only on chat nodes. */
  chatId?: string | null;
}

export interface TreeNode extends NodeSummary {
  children: TreeNode[];
}

export interface BreadcrumbItem {
  id: string;
  label: string;
  kind: "root" | "folder";
}

export interface ChatStats {
  charCount: number;
  wordCount: number;
  codeBlockCount: number;
  errorCount: number;
  commandCount: number;
  urlCount: number;
}

export type ArtifactType =
  | "code"
  | "error"
  | "command"
  | "url"
  | "decision"
  | "action_item"
  | "heading"
  | "question"
  | "answer"
  | "rejected"
  | "constraint"
  | "identifier";

export interface ChatArtifact {
  id: string;
  chatId: string;
  artifactType: ArtifactType;
  content: string;
  createdAt: string;
}

export interface ChatMeta {
  chatId: string;
  nodeId: string;
  rootId: string;
  title: string;
  source: string;
  tags: string | null;
  firstIdea: string | null;
  rawPath: string;
  briefPath: string | null;
  createdAt: string;
  updatedAt: string;
  stats: ChatStats;
}

export interface ChatDetail {
  meta: ChatMeta;
  briefMarkdown: string;
  artifacts: ChatArtifact[];
}

export interface SearchResult {
  chatId: string;
  nodeId: string;
  title: string;
  rootName: string;
  folderPath: string;
  snippet: string;
  createdAt: string;
}

export type SearchScopeKind = "global" | "root" | "folder";

export interface AppInfo {
  appName: string;
  version: string;
  dataDir: string;
  dbPath: string;
  filesDir: string;
  ftsEnabled: boolean;
  sqliteVersion: string;
}

export const ARTIFACT_LABELS: Record<ArtifactType, string> = {
  code: "Code",
  error: "Error",
  command: "Command",
  url: "URL",
  decision: "Decision",
  action_item: "Action Item",
  heading: "Heading",
  question: "Question",
  answer: "Key Point",
  rejected: "Rejected",
  constraint: "Constraint",
  identifier: "Identifier",
};

// ---------------------------------------------------------------------------
// AI-to-AI handoff packet
// ---------------------------------------------------------------------------

export type HandoffSlot =
  | "goal"
  | "decisions"
  | "rejected"
  | "identifiers"
  | "state"
  | "next_steps"
  | "open_questions"
  | "constraints";

export interface RejectedEntry {
  what: string;
  why?: string;
}

export interface SlotReport {
  slot: HandoffSlot;
  kept: number;
  dropped: number;
}

export interface HandoffPointer {
  chatId?: string;
  originalWords: number;
  originalChars: number;
  originalRetained: boolean;
}

export interface BudgetReport {
  tokenBudget: number;
  originalTokens: number;
  packetTokens: number;
  reductionPct: number;
  overBudget: boolean;
  slots: SlotReport[];
}

export interface HandoffPacket {
  version: number;
  title: string;
  goal?: string;
  decisions: string[];
  rejected: RejectedEntry[];
  identifiers: string[];
  state: string[];
  nextSteps: string[];
  openQuestions: string[];
  constraints: string[];
  pointer: HandoffPointer;
  budget: BudgetReport;
}

export interface HandoffExport {
  /** Paste-ready block for another AI's chat box. */
  rendered: string;
  /** `.strawberry.json` interchange form. */
  json: string;
  packet: HandoffPacket;
}

export const SLOT_LABELS: Record<HandoffSlot, string> = {
  goal: "Goal",
  decisions: "Decided",
  rejected: "Rejected",
  identifiers: "Identifiers",
  state: "State",
  next_steps: "Next",
  open_questions: "Open",
  constraints: "Rules",
};

// ---------------------------------------------------------------------------
// Screen Memory / Screenshot Recall
// ---------------------------------------------------------------------------

export interface ScreenConfig {
  intervalSecs: number;
  minChangeThreshold: number;
  enableOcr: boolean;
  enableEmbeddings: boolean;
  blocklist: string[];
  maxWidth: number;
  maxHeight: number;
  jpegQuality: number;
}

export interface ScreenFrame {
  id: number;
  ts: number;
  appName: string | null;
  windowTitle: string | null;
  filePath: string;
  width: number;
  height: number;
  byteSize: number;
  perceptualHash: string;
  ocrText: string | null;
  isBlurred: boolean;
  thumbnailPath: string | null;
  createdAt: string;
}

export interface ScreenSearchHit {
  id: number;
  ts: number;
  appName: string | null;
  windowTitle: string | null;
  filePath: string;
  perceptualHash: string;
  snippet: string;
  score: number;
}

export interface ScreenBlocklistItem {
  id: number;
  pattern: string;
  addedAt: string;
  reason: string | null;
}

export interface ScreenBlocklist {
  pattern: string;
  reason?: string;
}

// ---------------------------------------------------------------------------
// Ambient Memory & Symbolic Graph
// ---------------------------------------------------------------------------

export interface AmbientEvent {
  id: string;
  eventType: string;
  title: string;
  summary: string;
  sourceApp?: string | null;
  metadata?: string | null;
  createdAt: string;
}

export interface AmbientStats {
  totalEvents: number;
  clipEvents: number;
  screenEvents: number;
  astEvents: number;
  platform: string;
}

export interface SymbolItem {
  kind: "Function" | "ClassOrStruct" | "InterfaceOrTrait" | "Import" | "ErrorOrThrow";
  name: string;
  signature: string;
  line: number;
}

export interface SymbolicAnalysis {
  language: string;
  totalLines: number;
  imports: string[];
  functions: SymbolItem[];
  typesOrClasses: SymbolItem[];
  errorPoints: SymbolItem[];
}

export interface DeterministicReport {
  timestamp: string;
  platform: string;
  totalEventsAnalyzed: number;
  activeLanguages: string[];
  extractedSymbols: number;
  summaryMarkdown: string;
}

// ---------------------------------------------------------------------------
// Calendar & Workshop Events
// ---------------------------------------------------------------------------

export interface CalendarEvent {
  id: string;
  title: string;
  description?: string | null;
  startAt: string;
  endAt: string;
  timezone: string;
  category: string;
  sourceUrl?: string | null;
  location?: string | null;
  isAllDay: boolean;
  certificateOffered: boolean;
  registrationRequired: boolean;
  recurrence: string;
  recurrenceEnd?: string | null;
  color?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface EventReminder {
  id: string;
  eventId: string;
  minutesBefore: number;
  enabled: boolean;
  triggered: boolean;
}

/** Payload accepted by `create_calendar_event` / `update_calendar_event`. */
export interface CalendarEventInput {
  title: string;
  description?: string | null;
  startAt: string;
  endAt: string;
  timezone?: string | null;
  category?: string | null;
  sourceUrl?: string | null;
  location?: string | null;
  isAllDay?: boolean;
  certificateOffered?: boolean;
  registrationRequired?: boolean;
  /** `none` | `daily` | `weekly` | `monthly` | `yearly`. */
  recurrence?: string;
  /** Inclusive last day an occurrence may start on (YYYY-MM-DD). */
  recurrenceEnd?: string | null;
  color?: string | null;
  /** Minutes-before-start reminder offsets; replaces existing reminders when set. */
  reminderMinutes?: number[];
}

// ---------------------------------------------------------------------------
// Workspace Resume v0.1
// ---------------------------------------------------------------------------

export type WorkspaceSessionStatus =
  | "capturing"
  | "frozen"
  | "restoring"
  | "restored"
  | "partial"
  | "failed";

export type WorkspaceItemStatus =
  | "pending"
  | "launching"
  | "restored"
  | "skipped"
  | "failed";

export interface WorkspaceItem {
  id: string;
  sessionId: string;
  itemType: string;
  appName?: string | null;
  processName?: string | null;
  windowTitle?: string | null;
  windowGeometry?: string | null;
  workspace?: string | null;
  cwd?: string | null;
  command?: string | null;
  browserUrl?: string | null;
  browserTitle?: string | null;
  restoreStrategy: string;
  restoreStatus: WorkspaceItemStatus;
  errorMessage?: string | null;
  actionType?: string | null;
  actionTarget?: string | null;
  actionPayload?: string | null;
  displayLabel?: string | null;
  lastActionAt?: number | null;
  createdAt: number;
}

export interface WorkspaceSession {
  id: string;
  name: string;
  createdAt: number;
  frozenAt?: number | null;
  resumedAt?: number | null;
  status: WorkspaceSessionStatus;
  trigger: string;
  metadataJson?: string | null;
  items: WorkspaceItem[];
}

export interface ActionResult {
  success: boolean;
  itemId: string;
  message: string;
}

// Re-export namespaces from api.ts so legacy imports
// `import type { autonomy } from "../lib/types"` keep working.
export type { autonomy, alpha, ghost, wellness } from "./api";

