import { invoke } from "@tauri-apps/api/core";

export type TimeMode = "all_day" | "floating" | "zoned";
export type TaskStatus = "todo" | "in_progress" | "snoozed" | "blocked" | "completed" | "canceled";
export type Priority = "p0" | "p1" | "p2" | "p3";
export type RecurringEditScope = "only_this" | "this_and_future" | "entire_series";

export interface TaskItem {
  id: string | null;
  seriesId: string | null;
  occurrenceKey: string | null;
  title: string;
  notes: string | null;
  priority: Priority;
  estimatedMinutes: number | null;
  completionCriteria: string | null;
  status: TaskStatus;
  timeMode: TimeMode;
  scheduledDate: string | null;
  scheduledLocal: string | null;
  scheduledUtc: string | null;
  tzid: string | null;
  dstAdjusted: boolean;
  isVirtual: boolean;
  version: number;
  seriesVersion: number | null;
  progressPercent: number | null;
  progressNote: string | null;
  blockedReason: string | null;
  snoozedUntilUtc: string | null;
}

export interface CreateTaskInput {
  title: string;
  notes?: string;
  date: string;
  time?: string;
  timeMode?: "floating" | "zoned";
  tzid?: string;
  priority: Priority;
  estimatedMinutes?: number;
  completionCriteria?: string;
  reminderMinutesBefore?: number;
}

export interface CreateRecurringTaskInput {
  title: string;
  notes?: string;
  startDate: string;
  time?: string;
  timeMode?: "floating" | "zoned";
  tzid?: string;
  priority: Priority;
  estimatedMinutes?: number;
  completionCriteria?: string;
  rrule: string;
  reminderMinutesBefore?: number;
}

export interface RuntimeInfo {
  appVersion: string;
  dataDirectory: string;
  databaseEngine: string;
  recurrenceEngine: string;
  timezoneDatabase: string;
  agentBridge: string;
}

export interface TrashItem {
  id: string;
  deletedAtUtc: string;
  purgeAfterUtc: string;
  task: TaskItem;
}

export interface NotificationItem {
  id: string;
  reminderId: string;
  taskId: string;
  title: string;
  scheduledForUtc: string;
  deliveredAtUtc: string;
  taskStatus: string;
  isRead: boolean;
}

export interface Preferences {
  theme: "system" | "light" | "dark";
  weekStartsOn: "monday" | "sunday";
  clockFormat: "24h" | "12h";
  closeToTray: boolean;
  notificationsEnabled: boolean;
  appTimezone: string;
}

export interface BackupInfo {
  name: string;
  createdAtUtc: string;
  sizeBytes: number;
}

export interface WatchFieldItem {
  id: string;
  name: string;
  description: string;
  includeTerms: string[];
  excludeTerms: string[];
  regions: string[];
  languages: string[];
  maxItems: number;
  readingMinutes: number;
  relevanceWeight: number;
  recencyWeight: number;
  authorityWeight: number;
  heatWeight: number;
  breakingAlerts: boolean;
  enabled: boolean;
  sourceCount: number;
  lastSuccessAtUtc: string | null;
}

export interface WatchSourceItem {
  id: string;
  fieldId: string;
  name: string;
  url: string;
  sourceType: "rss" | "atom" | "api" | "page";
  selector: string | null;
  authority: number;
  enabled: boolean;
  lastSuccessAtUtc: string | null;
  lastError: string | null;
  sortOrder: number;
}

export interface NewsItem {
  id: string;
  fieldId: string;
  sourceId: string;
  sourceName: string;
  canonicalUrl: string;
  title: string;
  summary: string;
  publishedAtUtc: string | null;
  informationKind: string;
  score: number;
  isRead: boolean;
  isSaved: boolean;
  feedback: string | null;
  uncertainty: string | null;
}

export interface RefreshResult {
  sourceCount: number;
  succeeded: number;
  failed: number;
  newItems: number;
}

export interface LearningGoalItem {
  id: string;
  name: string;
  purpose: string;
  currentLevel: string;
  targetLevel: string;
  targetDate: string | null;
  weeklyMinutes: number;
  dailyMinutes: number;
  language: string;
  resourcePreferences: string[];
  budgetMode: "free_first" | "paid_allowed";
  autoAddLessons: boolean;
  status: string;
  nodeCount: number;
  dueCount: number;
}

export interface KnowledgeNodeItem {
  id: string;
  goalId: string;
  name: string;
  plainExplanation: string;
  stageOutcome: string;
  estimatedMinutes: number | null;
  prerequisiteIds: string[];
  status: string;
  masteryScore: number;
  quizCount: number;
  dueCount: number;
}

export interface LearningResourceItem {
  id: string;
  nodeId: string;
  title: string;
  author: string | null;
  publishedDate: string | null;
  resourceType: string;
  durationMinutes: number | null;
  difficulty: string | null;
  cost: string | null;
  versionFit: string | null;
  url: string;
  recommendationReason: string;
}

export interface QuizItemView {
  id: string;
  nodeId: string;
  nodeName: string;
  questionType: string;
  prompt: string;
  options: string[];
  explanationAfterSubmit: string | null;
  difficulty: number;
  dueUtc: string;
  isDue: boolean;
}

export interface QuizResult {
  attemptId: string;
  isCorrect: boolean;
  correctAnswer: string[];
  explanation: string;
  nextDueUtc: string;
  rating: number;
  masteryScore: number;
  nodeStatus: string;
}

export interface RuntimeProbe {
  provider: string;
  state: "not_installed" | "not_authenticated" | "ready";
  version: string | null;
  message: string;
  isolatedHome: string;
  capabilities: string[];
}

export interface BriefResult {
  oneSentenceGoal: string;
  priorities: { taskId: string; reason: string; firstStep: string }[];
  risks: string[];
  deferCandidates: string[];
  generatedAtUtc: string;
  provider: string;
}

export interface AgentBudgetSettings {
  enabled: boolean;
  dailyRunLimit: number;
  monthlyRunLimit: number;
  budgetMode: "saving" | "standard" | "deep";
}

export interface AutomationSettings {
  morningEnabled: boolean;
  morningTime: string;
  eveningEnabled: boolean;
  eveningTime: string;
  weeklyEnabled: boolean;
  weeklyWeekday: number;
  weeklyTime: string;
  intelligenceEnabled: boolean;
  intelligenceIntervalHours: number;
  lessonEnabled: boolean;
  lessonTime: string;
}

export interface DigestItem {
  id: string;
  digestType: "morning" | "evening" | "weekly" | "industry";
  periodStart: string;
  periodEnd: string;
  content: {
    headline?: string;
    priorities?: Array<Record<string, unknown>>;
    risks?: Array<Record<string, unknown>>;
    deferCandidates?: Array<Record<string, unknown>>;
    industry?: Array<Record<string, unknown>>;
    statistics?: Record<string, number>;
    needsReview?: Array<Record<string, unknown>>;
    completionRate?: number | null;
    plannedMinutes?: number;
    industryTrends?: Array<Record<string, unknown>>;
    learning?: Record<string, number>;
    nextWeekCandidates?: Array<Record<string, unknown>>;
    items?: Array<Record<string, unknown>>;
  };
  generator: string;
  createdAtUtc: string;
}

export interface LessonPlanItem {
  id: string;
  nodeId: string | null;
  itemKind: "review" | "learn" | "practice" | "quiz";
  title: string;
  minutes: number;
  sortOrder: number;
}

export interface LessonPlan {
  id: string;
  planDate: string;
  goalId: string;
  goalName: string;
  budgetMinutes: number;
  plannedMinutes: number;
  taskLoad: "light" | "normal" | "heavy";
  status: "suggested" | "approved" | "completed" | "dismissed";
  version: number;
  items: LessonPlanItem[];
}

export interface ProposalPreview {
  id: string;
  proposalType: string;
  diff: { planId?: string; tasks?: Array<Record<string, unknown>> };
  status: string;
  entityVersion: number | null;
  expiresAtUtc: string;
  approvalToken?: string;
}

export interface LogItem {
  id: string;
  category: string;
  level: string;
  eventName: string;
  correlationId: string;
  message: string;
  createdAtUtc: string;
}

export const taskApi = {
  list(rangeStart: string, rangeEnd: string, includeCompleted = true) {
    return invoke<TaskItem[]>("list_tasks", {
      input: { rangeStart, rangeEnd, includeCompleted },
    });
  },
  create(input: CreateTaskInput) {
    return invoke<TaskItem>("create_task", { input });
  },
  createRecurring(input: CreateRecurringTaskInput) {
    return invoke<string>("create_recurring_task", { input });
  },
  setStatus(
    task: TaskItem,
    status: TaskStatus,
    details?: { blockedReason?: string; snoozedMinutes?: number },
  ) {
    return invoke<TaskItem>("set_task_status", {
      input: {
        taskId: task.id,
        seriesId: task.seriesId,
        occurrenceKey: task.occurrenceKey,
        status,
        expectedVersion: task.version || undefined,
        ...details,
      },
    });
  },
  update(task: TaskItem, input: CreateTaskInput, preserveReminder = false) {
    return invoke<TaskItem>("update_task", {
      input: {
        taskId: task.id,
        seriesId: task.seriesId,
        occurrenceKey: task.occurrenceKey,
        expectedVersion: task.version || undefined,
        preserveReminder,
        ...input,
      },
    });
  },
  updateSeries(task: TaskItem, input: CreateTaskInput, editScope: RecurringEditScope) {
    if (!task.seriesId || !task.occurrenceKey || !task.seriesVersion) {
      return Promise.reject(new Error("Recurring task metadata is incomplete"));
    }
    return invoke<string>("update_recurring_series", {
      input: {
        taskId: task.id,
        seriesId: task.seriesId,
        occurrenceKey: task.occurrenceKey,
        expectedVersion: task.version || undefined,
        expectedSeriesVersion: task.seriesVersion,
        editScope,
        ...input,
      },
    });
  },
  delete(task: TaskItem) {
    return invoke<string>("delete_task", {
      input: {
        taskId: task.id,
        seriesId: task.seriesId,
        occurrenceKey: task.occurrenceKey,
        expectedVersion: task.version || undefined,
      },
    });
  },
  listTrash() {
    return invoke<TrashItem[]>("list_trash");
  },
  restore(trashId: string) {
    return invoke<TaskItem>("restore_task", { trashId });
  },
  partial(task: TaskItem, progressPercent: number | undefined, progressNote: string) {
    return invoke<TaskItem>("record_partial_progress", {
      input: {
        taskId: task.id,
        seriesId: task.seriesId,
        occurrenceKey: task.occurrenceKey,
        expectedVersion: task.version || undefined,
        progressPercent,
        progressNote,
      },
    });
  },
  reviewNotStarted(task: TaskItem) {
    return invoke<TaskItem>("record_not_started_review", {
      input: {
        taskId: task.id,
        seriesId: task.seriesId,
        occurrenceKey: task.occurrenceKey,
        expectedVersion: task.version || undefined,
      },
    });
  },
  runtimeInfo() {
    return invoke<RuntimeInfo>("runtime_info");
  },
};

export const notificationApi = {
  list(limit = 30) {
    return invoke<NotificationItem[]>("list_notifications", { limit });
  },
  pause(mode: "one_hour" | "today" | "clear") {
    return invoke<string | null>("pause_notifications", { mode });
  },
};

export const dataApi = {
  preferences() {
    return invoke<Preferences>("get_preferences");
  },
  savePreferences(input: Preferences) {
    return invoke<Preferences>("save_preferences", { input });
  },
  startupEnabled() {
    return invoke<boolean>("get_startup_enabled");
  },
  setStartupEnabled(enabled: boolean) {
    return invoke<boolean>("set_startup_enabled", { enabled });
  },
  backups() {
    return invoke<BackupInfo[]>("list_backups");
  },
  createBackup() {
    return invoke<BackupInfo>("create_backup");
  },
  restoreBackup(name: string) {
    return invoke<string>("restore_backup", { name });
  },
  exportData() {
    return invoke<{ path: string; exportedAtUtc: string }>("export_data");
  },
  thirdPartyNotices() {
    return invoke<string>("third_party_notices");
  },
};

export const intelligenceApi = {
  fields() {
    return invoke<WatchFieldItem[]>("list_watch_fields");
  },
  createField(input: {
    name: string;
    description: string;
    includeTerms: string[];
    excludeTerms: string[];
    regions?: string[];
    languages?: string[];
    maxItems?: number;
    readingMinutes?: number;
    relevanceWeight?: number;
    recencyWeight?: number;
    authorityWeight?: number;
    heatWeight?: number;
    breakingAlerts?: boolean;
  }) {
    return invoke<WatchFieldItem>("create_watch_field", { input });
  },
  enableField(fieldId: string, enabled: boolean) {
    return invoke<void>("set_watch_field_enabled", { fieldId, enabled });
  },
  deleteField(fieldId: string) {
    return invoke<void>("delete_watch_field", { fieldId });
  },
  reorderFields(orderedIds: string[]) {
    return invoke<void>("reorder_watch_fields", { orderedIds });
  },
  sources(fieldId: string) {
    return invoke<WatchSourceItem[]>("list_watch_sources", { fieldId });
  },
  createSource(input: {
    fieldId: string;
    name: string;
    url: string;
    sourceType?: "rss" | "atom" | "api" | "page";
    selector?: string;
    authority?: number;
  }) {
    return invoke<WatchSourceItem>("create_watch_source", { input });
  },
  enableSource(sourceId: string, enabled: boolean) {
    return invoke<void>("set_watch_source_enabled", { sourceId, enabled });
  },
  deleteSource(sourceId: string) {
    return invoke<void>("delete_watch_source", { sourceId });
  },
  reorderSources(fieldId: string, orderedIds: string[]) {
    return invoke<void>("reorder_watch_sources", { fieldId, orderedIds });
  },
  refresh(fieldId?: string) {
    return invoke<RefreshResult>("refresh_intelligence", { fieldId });
  },
  news(fieldId?: string, filter = "latest") {
    return invoke<NewsItem[]>("list_news", { fieldId, filter });
  },
  feedback(newsId: string, action: "read" | "saved" | "not_interested", value: boolean) {
    return invoke<void>("set_news_feedback", { input: { newsId, action, value } });
  },
};

export const learningApi = {
  goals() {
    return invoke<LearningGoalItem[]>("list_learning_goals");
  },
  createGoal(input: {
    name: string;
    purpose: string;
    currentLevel: string;
    targetLevel: string;
    targetDate?: string;
    weeklyMinutes?: number;
    dailyMinutes?: number;
    language?: string;
    resourcePreferences?: string[];
    budgetMode?: "free_first" | "paid_allowed";
    autoAddLessons?: boolean;
  }) {
    return invoke<LearningGoalItem>("create_learning_goal", { input });
  },
  nodes(goalId: string) {
    return invoke<KnowledgeNodeItem[]>("list_knowledge_nodes", { goalId });
  },
  createNode(input: {
    goalId: string;
    name: string;
    plainExplanation: string;
    stageOutcome: string;
    estimatedMinutes?: number;
    prerequisiteIds: string[];
  }) {
    return invoke<KnowledgeNodeItem>("create_knowledge_node", { input });
  },
  resources(nodeId: string) {
    return invoke<LearningResourceItem[]>("list_learning_resources", { nodeId });
  },
  createResource(input: {
    nodeId: string;
    title: string;
    author?: string;
    publishedDate?: string;
    resourceType: string;
    durationMinutes?: number;
    difficulty?: string;
    cost?: string;
    versionFit?: string;
    url: string;
    recommendationReason: string;
  }) {
    return invoke<LearningResourceItem>("create_learning_resource", { input });
  },
  createQuiz(input: {
    nodeId: string;
    questionType: "single" | "multiple" | "true_false";
    prompt: string;
    options: string[];
    correctAnswer: string[];
    explanation: string;
    difficulty?: number;
  }) {
    return invoke<QuizItemView>("create_quiz_item", { input });
  },
  quizzes(goalId?: string, dueOnly = false) {
    return invoke<QuizItemView[]>("list_quiz_items", { goalId, dueOnly });
  },
  submit(quizId: string, answer: string[], confidence: "sure" | "unsure" | "skipped") {
    return invoke<QuizResult>("submit_quiz", {
      input: { quizId, answer, confidence },
    });
  },
};

export const runtimeApi = {
  probe() {
    return invoke<RuntimeProbe>("probe_agent_runtime");
  },
  login() {
    return invoke<void>("start_agent_login");
  },
  logout() {
    return invoke<void>("logout_agent_runtime");
  },
  morningBrief(date: string, locale: string) {
    return invoke<BriefResult>("generate_morning_brief", { input: { date, locale } });
  },
  cancelRun() {
    return invoke<boolean>("cancel_agent_run");
  },
  budget() {
    return invoke<AgentBudgetSettings>("get_agent_budget");
  },
  saveBudget(input: AgentBudgetSettings) {
    return invoke<AgentBudgetSettings>("save_agent_budget", { input });
  },
};

export const supervisionApi = {
  automationSettings() {
    return invoke<AutomationSettings>("get_automation_settings");
  },
  saveAutomationSettings(input: AutomationSettings) {
    return invoke<AutomationSettings>("save_automation_settings", { input });
  },
  generateDigest(digestType: DigestItem["digestType"], date: string) {
    return invoke<DigestItem>("generate_digest", { digestType, date });
  },
  digests(limit = 30) {
    return invoke<DigestItem[]>("list_digests", { limit });
  },
  generateLessons(date: string) {
    return invoke<LessonPlan[]>("generate_daily_lesson", { date });
  },
  lessons(date: string) {
    return invoke<LessonPlan[]>("list_daily_lessons", { date });
  },
  proposeLessonTasks(planId: string) {
    return invoke<ProposalPreview>("propose_lesson_tasks", { planId });
  },
  applyProposal(proposalId: string, approvalToken: string) {
    return invoke<string[]>("apply_proposal", { proposalId, approvalToken });
  },
  rejectProposal(proposalId: string) {
    return invoke<void>("reject_proposal", { proposalId });
  },
  recentFailures(limit = 50) {
    return invoke<LogItem[]>("list_recent_failures", { limit });
  },
};
