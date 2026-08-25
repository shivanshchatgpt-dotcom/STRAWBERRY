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
  | "answer";

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
};
