export interface AgentTaskSnapshot {
  id: string;
  title: string;
  date: string;
  time?: string | null;
  status: "todo" | "in_progress" | "snoozed" | "blocked" | "completed" | "canceled";
  priority: "p0" | "p1" | "p2" | "p3";
}

export const READ_TOOL_NAMES = [
  "get_user_profile",
  "get_tasks",
  "get_task_events",
  "get_watch_fields",
  "get_news_history",
  "get_learning_state",
] as const;

export type ReadToolName = (typeof READ_TOOL_NAMES)[number];

export interface AgentTaskEventSnapshot {
  id: string;
  taskId: string;
  eventType: string;
  occurredAtUtc: string;
}

export interface AgentWatchFieldSnapshot {
  id: string;
  name: string;
  description?: string | null;
  includeKeywords: string[];
  excludeKeywords: string[];
  regions: string[];
  languages: string[];
}

export interface AgentNewsHistorySnapshot {
  id: string;
  fieldId: string;
  eventFingerprint: string;
  title: string;
  canonicalUrl: string;
  publishedAtUtc?: string | null;
  isRead: boolean;
  isSaved: boolean;
  isNotInterested: boolean;
}

export interface AgentLearningNodeSnapshot {
  id: string;
  goalId: string;
  title: string;
  mastery: number;
  nextReviewAtUtc?: string | null;
}

export interface AgentUserProfileSnapshot {
  locale: "zh-CN" | "en-US";
  timezone: string;
  clockFormat: "12h" | "24h";
}

export interface AgentSnapshot {
  generatedAtUtc: string;
  tasks: AgentTaskSnapshot[];
  userProfile?: AgentUserProfileSnapshot;
  taskEvents?: AgentTaskEventSnapshot[];
  watchFields?: AgentWatchFieldSnapshot[];
  newsHistory?: AgentNewsHistorySnapshot[];
  learningState?: AgentLearningNodeSnapshot[];
}

export interface AgentBridgeProbe {
  bridgeVersion: string;
  nodeVersion: string;
  transport: "stdio";
  databaseAccess: false;
  tools: string[];
}
