import {
  Badge,
  Button,
  Card,
  Checkbox,
  Input,
  MessageBar,
  MessageBarBody,
  ProgressBar,
  Spinner,
  Text,
  Textarea,
  Title1,
  Title2,
  Tooltip,
} from "@fluentui/react-components";
import { FormEvent, lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  CreateRecurringTaskInput,
  CreateTaskInput,
  DigestItem,
  NotificationItem,
  Preferences,
  Priority,
  RecurringEditScope,
  RuntimeInfo,
  TaskItem,
  TaskStatus,
  notificationApi,
  dataApi,
  taskApi,
  supervisionApi,
} from "./api";
import { addDays, formatDate, parseDate, startOfWeek, taskDate, taskTime } from "./date";

const IntelligenceView = lazy(() =>
  import("./FeatureViews").then((module) => ({ default: module.IntelligenceView })),
);
const LearningView = lazy(() =>
  import("./FeatureViews").then((module) => ({ default: module.LearningView })),
);
const ProductSettingsView = lazy(() =>
  import("./FeatureViews").then((module) => ({ default: module.SettingsView })),
);
const BriefCard = lazy(() =>
  import("./FeatureViews").then((module) => ({ default: module.BriefCard })),
);

type View = "today" | "calendar" | "review" | "intelligence" | "learning" | "settings";
type Repeat = "none" | "daily" | "weekdays" | "weekly" | "monthly";
type CalendarMode = "day" | "week" | "month";

const views: View[] = ["today", "calendar", "review", "intelligence", "learning", "settings"];

function recurrenceRule(repeat: Repeat): string | null {
  return {
    none: null,
    daily: "FREQ=DAILY",
    weekdays: "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
    weekly: "FREQ=WEEKLY",
    monthly: "FREQ=MONTHLY",
  }[repeat];
}

export function App() {
  const { i18n, t } = useTranslation();
  const today = useMemo(() => formatDate(new Date()), []);
  const [view, setView] = useState<View>("today");
  const [anchorDate, setAnchorDate] = useState(today);
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [composerOpen, setComposerOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfo | null>(null);
  const [editingTask, setEditingTask] = useState<TaskItem | null>(null);
  const [notifications, setNotifications] = useState<NotificationItem[]>([]);
  const [notificationOpen, setNotificationOpen] = useState(false);
  const [firstRun, setFirstRun] = useState(() => localStorage.getItem("pga-onboarding") !== "done");
  const [preferences, setPreferences] = useState<Preferences>({
    theme: "system",
    weekStartsOn: "monday",
    clockFormat: "24h",
    closeToTray: true,
    notificationsEnabled: true,
    appTimezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
  });

  const loadTasks = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const start = [addDays(today, -31), addDays(anchorDate, -45)].sort()[0];
      const end = [addDays(today, 91), addDays(anchorDate, 45)].sort()[1];
      setTasks(await taskApi.list(start, end, true));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, [anchorDate, today]);

  useEffect(() => void loadTasks(), [loadTasks]);

  useEffect(() => {
    const reload = () => void loadTasks();
    window.addEventListener("pga-tasks-changed", reload);
    return () => window.removeEventListener("pga-tasks-changed", reload);
  }, [loadTasks]);

  useEffect(() => {
    void dataApi
      .preferences()
      .then((loaded) => {
        setPreferences(loaded);
        localStorage.setItem("pga-theme", loaded.theme);
        window.dispatchEvent(new CustomEvent("pga-theme-changed", { detail: loaded.theme }));
      })
      .catch(() => undefined);
    const onPreferences = (event: Event) =>
      setPreferences((event as CustomEvent<Preferences>).detail);
    window.addEventListener("pga-preferences-changed", onPreferences);
    return () => window.removeEventListener("pga-preferences-changed", onPreferences);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisteners: UnlistenFn[] = [];
    void Promise.all([
      listen("tray-show-today", () => {
        setView("today");
        setAnchorDate(today);
      }),
      listen("tray-quick-add", () => {
        setView("today");
        setAnchorDate(today);
        setEditingTask(null);
        setComposerOpen(true);
      }),
    ])
      .then((registered) => {
        if (disposed) registered.forEach((unlisten) => unlisten());
        else unlisteners = registered;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [today]);

  useEffect(() => {
    const syncSystemTimezone = () => {
      const detected = Intl.DateTimeFormat().resolvedOptions().timeZone;
      if (!detected || detected === preferences.appTimezone) return;
      const next = { ...preferences, appTimezone: detected };
      void dataApi
        .savePreferences(next)
        .then((saved) => {
          setPreferences(saved);
          window.dispatchEvent(new CustomEvent("pga-preferences-changed", { detail: saved }));
        })
        .catch((reason) => setError(String(reason)));
    };
    syncSystemTimezone();
    const timer = window.setInterval(syncSystemTimezone, 60_000);
    return () => window.clearInterval(timer);
  }, [preferences]);

  useEffect(() => {
    void notificationApi
      .list()
      .then(setNotifications)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (view === "settings" && !runtimeInfo) {
      void taskApi
        .runtimeInfo()
        .then(setRuntimeInfo)
        .catch((reason) => setError(String(reason)));
    }
  }, [runtimeInfo, view]);

  async function changeLanguage() {
    const nextLanguage = i18n.resolvedLanguage === "zh-CN" ? "en-US" : "zh-CN";
    await i18n.changeLanguage(nextLanguage);
    localStorage.setItem("pga-language", nextLanguage);
    document.documentElement.lang = nextLanguage;
    document.title = t("appName", { lng: nextLanguage });
  }

  async function saveTask(input: CreateTaskInput, repeat: Repeat, editScope: RecurringEditScope) {
    setSaving(true);
    setError(null);
    try {
      const rrule = recurrenceRule(repeat);
      if (editingTask) {
        if (editingTask.seriesId && editScope !== "only_this") {
          await taskApi.updateSeries(editingTask, input, editScope);
        } else {
          await taskApi.update(editingTask, input);
        }
      } else if (rrule) {
        const recurring: CreateRecurringTaskInput = {
          title: input.title,
          notes: input.notes,
          startDate: input.date,
          time: input.time,
          timeMode: input.timeMode,
          tzid: input.tzid,
          priority: input.priority,
          estimatedMinutes: input.estimatedMinutes,
          completionCriteria: input.completionCriteria,
          reminderMinutesBefore: input.reminderMinutesBefore,
          rrule,
        };
        await taskApi.createRecurring(recurring);
      } else {
        await taskApi.create(input);
      }
      setComposerOpen(false);
      setEditingTask(null);
      await loadTasks();
    } finally {
      setSaving(false);
    }
  }

  const reportError = useCallback((reason: unknown) => setError(String(reason)), []);

  async function deleteTask(task: TaskItem) {
    if (!window.confirm(t("confirmDeleteTask", { title: task.title }))) return;
    setError(null);
    try {
      await taskApi.delete(task);
      await loadTasks();
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function partialTask(task: TaskItem) {
    const note = window.prompt(t("partialProgressPrompt"), task.progressNote ?? "");
    if (!note?.trim()) return;
    const rawPercent = window.prompt(
      t("progressPercentPrompt"),
      String(task.progressPercent ?? 50),
    );
    const percent = rawPercent?.trim() ? Number(rawPercent) : undefined;
    try {
      await taskApi.partial(task, percent, note.trim());
      await loadTasks();
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function duplicateTask(task: TaskItem) {
    setError(null);
    try {
      await taskApi.create({
        title: `${task.title} (${t("copySuffix")})`,
        notes: task.notes ?? undefined,
        date: taskDate(task),
        time: taskTime(task) ?? undefined,
        timeMode: task.timeMode as CreateTaskInput["timeMode"],
        tzid: task.timeMode === "zoned" ? (task.tzid ?? preferences.appTimezone) : undefined,
        priority: task.priority,
        estimatedMinutes: task.estimatedMinutes ?? undefined,
        completionCriteria: task.completionCriteria ?? undefined,
      });
      await loadTasks();
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function moveTask(task: TaskItem, date: string) {
    await taskApi.update(
      task,
      {
        title: task.title,
        notes: task.notes ?? undefined,
        date,
        time: taskTime(task) ?? undefined,
        timeMode: task.timeMode as CreateTaskInput["timeMode"],
        tzid: task.timeMode === "zoned" ? (task.tzid ?? preferences.appTimezone) : undefined,
        priority: task.priority,
        estimatedMinutes: task.estimatedMinutes ?? undefined,
        completionCriteria: task.completionCriteria ?? undefined,
      },
      true,
    );
  }

  async function toggleTask(task: TaskItem) {
    setError(null);
    const status = task.status === "completed" ? "todo" : "completed";
    setTasks((current) =>
      current.map((item) =>
        taskIdentity(item) === taskIdentity(task) ? { ...item, status } : item,
      ),
    );
    try {
      await taskApi.setStatus(task, status);
      await loadTasks();
    } catch (reason) {
      setError(String(reason));
      await loadTasks();
    }
  }

  async function changeTaskStatus(task: TaskItem, status: TaskStatus) {
    if (task.status === status) return;
    let details: { blockedReason?: string; snoozedMinutes?: number } | undefined;
    if (status === "blocked") {
      const reason = window.prompt(t("blockedReasonPrompt"), task.blockedReason ?? "");
      if (!reason?.trim()) return;
      details = { blockedReason: reason.trim() };
    }
    if (status === "snoozed") {
      const raw = window.prompt(t("snoozeMinutesPrompt"), "60");
      if (!raw) return;
      const minutes = Number(raw);
      if (!Number.isFinite(minutes) || minutes < 1 || minutes > 43_200) {
        setError(t("invalidSnoozeMinutes"));
        return;
      }
      details = { snoozedMinutes: Math.round(minutes) };
    }
    setError(null);
    try {
      await taskApi.setStatus(task, status, details);
      await loadTasks();
    } catch (reason) {
      setError(String(reason));
      await loadTasks();
    }
  }

  const todayTasks = tasks.filter((task) => taskDate(task) === today && task.status !== "canceled");
  const completed = todayTasks.filter((task) => task.status === "completed").length;

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label={t("primaryNavigation")}>
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            ↗
          </span>
          <div>
            <Text weight="semibold" size={400}>
              {t("appName")}
            </Text>
            <Text size={200} className="muted">
              {t("localFirst")}
            </Text>
          </div>
        </div>
        <nav className="nav-list">
          {views.map((item) => (
            <button
              type="button"
              className={`nav-item ${view === item ? "active" : ""}`}
              aria-current={view === item ? "page" : undefined}
              onClick={() => setView(item)}
              key={item}
            >
              <span className="nav-symbol" aria-hidden="true">
                {t(`navSymbol.${item}`)}
              </span>
              {t(`nav.${item}`)}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className="local-dot" aria-hidden="true" />
          <Text size={200}>{t("savedLocally")}</Text>
        </div>
      </aside>

      <main className="workspace">
        <header className="workspace-header">
          <div>
            <Text className="date-kicker">{localizedLongDate(today, i18n.resolvedLanguage)}</Text>
            <Title1 as="h1">{t(`page.${view}`)}</Title1>
          </div>
          <div className="header-actions">
            <Tooltip content={t("switchLanguage")} relationship="label">
              <Button appearance="subtle" onClick={changeLanguage}>
                {i18n.resolvedLanguage === "zh-CN" ? "EN" : "中"}
              </Button>
            </Tooltip>
            <div className="notification-anchor">
              <Button
                appearance="subtle"
                aria-label={t("notifications")}
                onClick={() => setNotificationOpen((value) => !value)}
              >
                ◉{notifications.length ? ` ${notifications.length}` : ""}
              </Button>
              {notificationOpen && (
                <NotificationPopover
                  notifications={notifications}
                  t={t}
                  onComplete={(notification) => {
                    const task = tasks.find((item) => item.id === notification.taskId);
                    if (task && task.status !== "completed") {
                      void changeTaskStatus(task, "completed").then(() =>
                        setNotifications((current) =>
                          current.map((item) =>
                            item.taskId === notification.taskId
                              ? { ...item, taskStatus: "completed" }
                              : item,
                          ),
                        ),
                      );
                    }
                  }}
                  onSnooze={(notification) => {
                    const task = tasks.find((item) => item.id === notification.taskId);
                    if (task && !["completed", "canceled"].includes(task.status)) {
                      void taskApi
                        .setStatus(task, "snoozed", { snoozedMinutes: 10 })
                        .then(async () => {
                          setNotifications((current) =>
                            current.map((item) =>
                              item.taskId === notification.taskId
                                ? { ...item, taskStatus: "snoozed" }
                                : item,
                            ),
                          );
                          await loadTasks();
                        })
                        .catch(reportError);
                    }
                  }}
                  onOpen={(notification) => {
                    const task = tasks.find((item) => item.id === notification.taskId);
                    setNotificationOpen(false);
                    setView(task ? "calendar" : "today");
                    if (task) setAnchorDate(taskDate(task));
                  }}
                />
              )}
            </div>
            {view !== "settings" && (
              <Button appearance="primary" onClick={() => setComposerOpen(true)}>
                ＋ {t("newTask")}
              </Button>
            )}
          </div>
        </header>

        {error && (
          <MessageBar intent="error" className="page-message">
            <MessageBarBody>{error}</MessageBarBody>
          </MessageBar>
        )}

        {loading ? (
          <div className="loading-state">
            <Spinner label={t("loadingLocalData")} />
          </div>
        ) : (
          <Suspense
            fallback={
              <div className="loading-state">
                <Spinner label={t("loadingLocalData")} />
              </div>
            }
          >
            {view === "today" && (
              <TodayView
                tasks={todayTasks}
                completed={completed}
                onToggle={(task) => void toggleTask(task)}
                onEdit={(task) => {
                  setEditingTask(task);
                  setComposerOpen(true);
                }}
                onDelete={(task) => void deleteTask(task)}
                onPartial={(task) => void partialTask(task)}
                onCopy={(task) => void duplicateTask(task)}
                onStatus={(task, status) => void changeTaskStatus(task, status)}
                onAdd={() => setComposerOpen(true)}
                t={t}
                locale={i18n.resolvedLanguage ?? "zh-CN"}
                clockFormat={preferences.clockFormat}
                reportError={reportError}
              />
            )}
            {view === "calendar" && (
              <CalendarView
                anchorDate={anchorDate}
                tasks={tasks}
                onChangeDate={setAnchorDate}
                onEdit={(task) => {
                  setEditingTask(task);
                  setComposerOpen(true);
                }}
                onCopy={(task) => void duplicateTask(task)}
                onMove={async (task, date) => {
                  try {
                    await moveTask(task, date);
                  } catch (reason) {
                    setError(String(reason));
                    throw reason;
                  }
                }}
                onReload={loadTasks}
                t={t}
                locale={i18n.resolvedLanguage}
                preferences={preferences}
              />
            )}
            {view === "review" && (
              <ReviewView
                tasks={tasks}
                today={today}
                t={t}
                reportError={reportError}
                onStatus={(task, status) => void changeTaskStatus(task, status)}
                onPartial={(task) => void partialTask(task)}
                onNotStarted={(task) =>
                  void taskApi.reviewNotStarted(task).then(loadTasks).catch(reportError)
                }
              />
            )}
            {view === "intelligence" && <IntelligenceView reportError={reportError} />}
            {view === "learning" && <LearningView reportError={reportError} />}
            {view === "settings" && (
              <ProductSettingsView
                info={runtimeInfo}
                onLanguage={changeLanguage}
                reportError={reportError}
              />
            )}
          </Suspense>
        )}
      </main>

      {composerOpen && (
        <TaskComposer
          initialDate={view === "calendar" ? anchorDate : today}
          saving={saving}
          initialTask={editingTask}
          onClose={() => {
            setComposerOpen(false);
            setEditingTask(null);
          }}
          onSave={(input, repeat, editScope) =>
            void saveTask(input, repeat, editScope).catch((reason) => setError(String(reason)))
          }
          t={t}
        />
      )}
      {firstRun && (
        <FirstRunWizard
          onFinish={async (preferences, language) => {
            await dataApi.savePreferences(preferences);
            localStorage.setItem("pga-theme", preferences.theme);
            localStorage.setItem("pga-language", language);
            localStorage.setItem("pga-onboarding", "done");
            window.dispatchEvent(
              new CustomEvent("pga-theme-changed", { detail: preferences.theme }),
            );
            await i18n.changeLanguage(language);
            document.documentElement.lang = language;
            document.title = t("appName", { lng: language });
            setFirstRun(false);
          }}
          reportError={reportError}
        />
      )}
    </div>
  );
}

function FirstRunWizard({
  onFinish,
  reportError,
}: {
  onFinish: (preferences: Preferences, language: "zh-CN" | "en-US") => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const { i18n, t } = useTranslation();
  const [language, setLanguage] = useState<"zh-CN" | "en-US">(
    i18n.resolvedLanguage === "en-US" ? "en-US" : "zh-CN",
  );
  const [theme, setTheme] = useState<Preferences["theme"]>("system");
  const [closeToTray, setCloseToTray] = useState(true);
  const [notificationsEnabled, setNotificationsEnabled] = useState(true);
  const [saving, setSaving] = useState(false);
  const systemTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai";
  return (
    <div className="modal-backdrop onboarding-backdrop">
      <section className="composer onboarding" role="dialog" aria-modal="true">
        <div className="onboarding-symbol" aria-hidden="true">
          ↗
        </div>
        <Title1 as="h1">{t("welcomeTitle")}</Title1>
        <Text className="muted">{t("welcomeBody")}</Text>
        <div className="onboarding-grid">
          <label>
            <span>{t("interfaceLanguage")}</span>
            <select
              value={language}
              onChange={(event) => setLanguage(event.target.value as "zh-CN" | "en-US")}
            >
              <option value="zh-CN">简体中文</option>
              <option value="en-US">English</option>
            </select>
          </label>
          <label>
            <span>{t("theme")}</span>
            <select
              value={theme}
              onChange={(event) => setTheme(event.target.value as Preferences["theme"])}
            >
              <option value="system">{t("themeSystem")}</option>
              <option value="light">{t("themeLight")}</option>
              <option value="dark">{t("themeDark")}</option>
            </select>
          </label>
        </div>
        <div className="onboarding-timezone">
          <Text weight="semibold">{t("detectedTimezone")}</Text>
          <Text className="muted">{systemTimeZone}</Text>
        </div>
        <label className="checkbox-label onboarding-check">
          <Checkbox
            checked={notificationsEnabled}
            onChange={(_, data) => setNotificationsEnabled(Boolean(data.checked))}
          />
          <span>
            <strong>{t("enableNotifications")}</strong>
            <Text size={200} className="muted">
              {t("enableNotificationsHint")}
            </Text>
          </span>
        </label>
        <label className="checkbox-label onboarding-check">
          <Checkbox
            checked={closeToTray}
            onChange={(_, data) => setCloseToTray(Boolean(data.checked))}
          />
          <span>
            <strong>{t("closeToTray")}</strong>
            <Text size={200} className="muted">
              {t("closeToTrayHint")}
            </Text>
          </span>
        </label>
        <MessageBar intent="info">
          <MessageBarBody>{t("welcomePrivacy")}</MessageBarBody>
        </MessageBar>
        <Button
          appearance="primary"
          size="large"
          disabled={saving}
          onClick={() => {
            setSaving(true);
            void onFinish(
              {
                theme,
                weekStartsOn: "monday",
                clockFormat: "24h",
                closeToTray,
                notificationsEnabled,
                appTimezone: systemTimeZone,
              },
              language,
            )
              .catch(reportError)
              .finally(() => setSaving(false));
          }}
        >
          {saving ? t("saving") : t("startUsing")}
        </Button>
      </section>
    </div>
  );
}

function TodayView({
  tasks,
  completed,
  onToggle,
  onEdit,
  onDelete,
  onPartial,
  onCopy,
  onStatus,
  onAdd,
  t,
  locale,
  clockFormat,
  reportError,
}: {
  tasks: TaskItem[];
  completed: number;
  onToggle: (task: TaskItem) => void;
  onEdit: (task: TaskItem) => void;
  onDelete: (task: TaskItem) => void;
  onPartial: (task: TaskItem) => void;
  onCopy: (task: TaskItem) => void;
  onStatus: (task: TaskItem, status: TaskStatus) => void;
  onAdd: () => void;
  t: Translate;
  locale: string;
  clockFormat: Preferences["clockFormat"];
  reportError: (error: unknown) => void;
}) {
  const focusMinutes = tasks.reduce((sum, task) => sum + (task.estimatedMinutes ?? 0), 0);
  const progress = tasks.length ? completed / tasks.length : 0;
  const activeP0 = tasks.filter(
    (task) => task.priority === "p0" && !["completed", "canceled"].includes(task.status),
  );
  const activeP1 = tasks.filter(
    (task) => task.priority === "p1" && !["completed", "canceled"].includes(task.status),
  );
  const overloaded = activeP0.length > 1 || activeP1.length > 2;
  return (
    <div className="page-stack">
      {overloaded && (
        <MessageBar intent="warning">
          <MessageBarBody>
            {t("priorityCapacityWarning", {
              p0: activeP0.length,
              p1: activeP1.length,
              candidates: [...activeP1.slice(2), ...activeP0.slice(1)]
                .map((task) => task.title)
                .join("、"),
            })}
          </MessageBarBody>
        </MessageBar>
      )}
      <section className="summary-grid" aria-label={t("todayOverview")}>
        <Card className="summary-card accent-card">
          <Text className="summary-label">{t("todayProgress")}</Text>
          <div className="summary-value">
            {completed}
            <span> / {tasks.length}</span>
          </div>
          <ProgressBar value={progress} thickness="large" />
        </Card>
        <Card className="summary-card">
          <Text className="summary-label">{t("remaining")}</Text>
          <div className="summary-value">{tasks.length - completed}</div>
          <Text className="muted">{t("keepScopeSmall")}</Text>
        </Card>
        <Card className="summary-card">
          <Text className="summary-label">{t("plannedFocus")}</Text>
          <div className="summary-value">
            {focusMinutes}
            <span> min</span>
          </div>
          <Text className="muted">{t("estimateNotDeadline")}</Text>
        </Card>
      </section>
      <section className="content-card" aria-labelledby="today-list-title">
        <div className="section-title-row">
          <div>
            <Title2 as="h2" id="today-list-title">
              {t("todayTasks")}
            </Title2>
            <Text className="muted">{t("todayTasksHint")}</Text>
          </div>
          <Button appearance="subtle" onClick={onAdd}>
            {t("quickAdd")}
          </Button>
        </div>
        {tasks.length ? (
          <div className="task-list">
            {tasks.map((task) => (
              <TaskRow
                key={taskIdentity(task)}
                task={task}
                onToggle={onToggle}
                onEdit={onEdit}
                onDelete={onDelete}
                onPartial={onPartial}
                onCopy={onCopy}
                onStatus={onStatus}
                t={t}
                locale={locale}
                clockFormat={clockFormat}
              />
            ))}
          </div>
        ) : (
          <EmptyState
            title={t("emptyToday")}
            body={t("emptyTodayBody")}
            action={t("addFirstTask")}
            onAction={onAdd}
          />
        )}
      </section>
      <LocalDigestCard date={formatDate(new Date())} reportError={reportError} />
      <BriefCard date={formatDate(new Date())} locale={locale} reportError={reportError} />
    </div>
  );
}

function CalendarView({
  anchorDate,
  tasks,
  onChangeDate,
  onEdit,
  onCopy,
  onMove,
  onReload,
  t,
  locale,
  preferences,
}: {
  anchorDate: string;
  tasks: TaskItem[];
  onChangeDate: (date: string) => void;
  onEdit: (task: TaskItem) => void;
  onCopy: (task: TaskItem) => void;
  onMove: (task: TaskItem, date: string) => Promise<void>;
  onReload: () => Promise<void>;
  t: Translate;
  locale?: string;
  preferences: Preferences;
}) {
  const [mode, setMode] = useState<CalendarMode>("week");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [batchDate, setBatchDate] = useState(anchorDate);
  const [moving, setMoving] = useState(false);
  const monthStart = `${anchorDate.slice(0, 7)}-01`;
  const rangeStart =
    mode === "day"
      ? anchorDate
      : mode === "week"
        ? startOfWeek(anchorDate, preferences.weekStartsOn)
        : startOfWeek(monthStart, preferences.weekStartsOn);
  const dayCount = mode === "day" ? 1 : mode === "week" ? 7 : 42;
  const days = Array.from({ length: dayCount }, (_, index) => addDays(rangeStart, index));
  const navigate = (direction: -1 | 1) => {
    if (mode === "month") onChangeDate(shiftMonth(anchorDate, direction));
    else onChangeDate(addDays(anchorDate, direction * (mode === "week" ? 7 : 1)));
  };
  async function moveSelected() {
    const chosen = tasks.filter((task) => selected.has(taskIdentity(task)));
    if (!chosen.length) return;
    setMoving(true);
    try {
      const outcomes = await Promise.allSettled(chosen.map((task) => onMove(task, batchDate)));
      const failed = outcomes.filter((outcome) => outcome.status === "rejected");
      await onReload();
      if (failed.length) throw new Error(t("batchMovePartialFailure", { count: failed.length }));
      setSelected(new Set());
    } finally {
      setMoving(false);
    }
  }
  return (
    <div className="page-stack">
      <div className="calendar-toolbar">
        <Button appearance="subtle" onClick={() => navigate(-1)}>
          ← {t("previousPeriod")}
        </Button>
        <Input type="date" value={anchorDate} onChange={(_, data) => onChangeDate(data.value)} />
        <Button appearance="subtle" onClick={() => navigate(1)}>
          {t("nextPeriod")} →
        </Button>
        <div className="calendar-mode" role="group" aria-label={t("calendarViewMode")}>
          {(["day", "week", "month"] as const).map((item) => (
            <Button
              size="small"
              appearance={mode === item ? "primary" : "subtle"}
              onClick={() => setMode(item)}
              key={item}
            >
              {t(`calendarMode.${item}`)}
            </Button>
          ))}
        </div>
      </div>
      {selected.size > 0 && (
        <div className="batch-toolbar">
          <Text>{t("tasksSelected", { count: selected.size })}</Text>
          <Input type="date" value={batchDate} onChange={(_, data) => setBatchDate(data.value)} />
          <Button
            appearance="primary"
            disabled={moving}
            onClick={() => void moveSelected().catch(() => undefined)}
          >
            {moving ? t("movingTasks") : t("moveSelectedTasks")}
          </Button>
          <Button onClick={() => setSelected(new Set())}>{t("clearSelection")}</Button>
        </div>
      )}
      <section className={`calendar-grid mode-${mode}`} aria-label={t("calendarRange")}>
        {days.map((date) => {
          const dayTasks = tasks.filter(
            (task) => taskDate(task) === date && task.status !== "canceled",
          );
          return (
            <div
              className={`day-column ${mode === "month" && date.slice(0, 7) !== anchorDate.slice(0, 7) ? "outside-month" : ""}`}
              key={date}
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => {
                event.preventDefault();
                const identity = event.dataTransfer.getData("text/pga-task");
                const task = tasks.find((item) => taskIdentity(item) === identity);
                if (!task || taskDate(task) === date) return;
                setMoving(true);
                void onMove(task, date)
                  .then(onReload)
                  .catch(() => undefined)
                  .finally(() => setMoving(false));
              }}
            >
              <div className="day-heading">
                <Text weight="semibold">{localizedWeekday(date, locale)}</Text>
                <span className="day-number">{parseDate(date).getDate()}</span>
              </div>
              <div className="day-items">
                {dayTasks.map((task) => (
                  <div
                    className={`calendar-task priority-${task.priority} ${task.status}`}
                    key={taskIdentity(task)}
                    draggable
                    onDragStart={(event) =>
                      event.dataTransfer.setData("text/pga-task", taskIdentity(task))
                    }
                  >
                    <Checkbox
                      checked={selected.has(taskIdentity(task))}
                      onChange={(_, data) => {
                        setSelected((current) => {
                          const next = new Set(current);
                          if (data.checked) next.add(taskIdentity(task));
                          else next.delete(taskIdentity(task));
                          return next;
                        });
                      }}
                      aria-label={t("selectTask", { title: task.title })}
                    />
                    <button type="button" onClick={() => onEdit(task)} title={t("editTask")}>
                      {taskTime(task) && (
                        <span>{displayTaskTime(task, preferences.clockFormat, locale)}</span>
                      )}
                      <strong title={task.title}>{task.title}</strong>
                    </button>
                    <button
                      type="button"
                      className="calendar-copy"
                      onClick={() => onCopy(task)}
                      aria-label={t("copyTask")}
                      title={t("copyTask")}
                    >
                      <span aria-hidden="true">⧉</span>
                    </button>
                  </div>
                ))}
                {!dayTasks.length && <span className="empty-day">—</span>}
              </div>
            </div>
          );
        })}
      </section>
      <Text size={200} className="muted">
        {t("virtualOccurrenceHint")}
      </Text>
    </div>
  );
}

function ReviewView({
  tasks,
  today,
  t,
  reportError,
  onStatus,
  onPartial,
  onNotStarted,
}: {
  tasks: TaskItem[];
  today: string;
  t: Translate;
  reportError: (error: unknown) => void;
  onStatus: (task: TaskItem, status: TaskStatus) => void;
  onPartial: (task: TaskItem) => void;
  onNotStarted: (task: TaskItem) => void;
}) {
  const [digests, setDigests] = useState<DigestItem[]>([]);
  const [digestBusy, setDigestBusy] = useState<string | null>(null);
  const loadDigests = useCallback(() => supervisionApi.digests().then(setDigests), []);
  useEffect(() => void loadDigests().catch(() => undefined), [loadDigests]);
  const start = addDays(today, -6);
  const week = tasks.filter((task) => taskDate(task) >= start && taskDate(task) <= today);
  const completed = week.filter((task) => task.status === "completed");
  const plannedMinutes = week.reduce((sum, task) => sum + (task.estimatedMinutes ?? 0), 0);
  const completion = week.length ? completed.length / week.length : 0;
  const todayCore = tasks.filter(
    (task) =>
      taskDate(task) === today &&
      ["p0", "p1"].includes(task.priority) &&
      !["completed", "canceled"].includes(task.status),
  );
  return (
    <div className="page-stack">
      <section className="summary-grid">
        <Card className="summary-card accent-card">
          <Text className="summary-label">{t("sevenDayCompletion")}</Text>
          <div className="summary-value">{Math.round(completion * 100)}%</div>
          <ProgressBar value={completion} thickness="large" />
        </Card>
        <Card className="summary-card">
          <Text className="summary-label">{t("tasksCompleted")}</Text>
          <div className="summary-value">{completed.length}</div>
          <Text className="muted">{t("outOfPlanned", { count: week.length })}</Text>
        </Card>
        <Card className="summary-card">
          <Text className="summary-label">{t("plannedInvestment")}</Text>
          <div className="summary-value">
            {plannedMinutes}
            <span> min</span>
          </div>
          <Text className="muted">{t("basedOnEstimates")}</Text>
        </Card>
      </section>
      <section className="content-card">
        <div className="section-title-row">
          <div>
            <Title2 as="h2">{t("eveningTaskCheck")}</Title2>
            <Text className="muted">{t("eveningTaskCheckHint")}</Text>
          </div>
        </div>
        {todayCore.length ? (
          <div className="review-outcomes">
            {todayCore.map((task) => (
              <div key={taskIdentity(task)}>
                <strong>{task.title}</strong>
                <div className="inline-actions">
                  <Button size="small" onClick={() => onStatus(task, "completed")}>
                    {t("reviewOutcome.completed")}
                  </Button>
                  <Button size="small" onClick={() => onPartial(task)}>
                    {t("reviewOutcome.partial")}
                  </Button>
                  <Button size="small" onClick={() => onNotStarted(task)}>
                    {t("reviewOutcome.notStarted")}
                  </Button>
                  <Button size="small" onClick={() => onStatus(task, "blocked")}>
                    {t("reviewOutcome.blocked")}
                  </Button>
                  <Button size="small" onClick={() => onStatus(task, "canceled")}>
                    {t("reviewOutcome.canceled")}
                  </Button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <Text className="muted">{t("noAmbiguousCoreTasks")}</Text>
        )}
      </section>
      <section className="content-card">
        <Title2 as="h2">{t("completedThisWeek")}</Title2>
        {completed.length ? (
          <div className="review-list">
            {completed.map((task) => (
              <div className="review-item" key={taskIdentity(task)}>
                <span aria-hidden="true">✓</span>
                <div>
                  <Text weight="semibold">{task.title}</Text>
                  <Text size={200} className="muted">
                    {taskDate(task)}
                  </Text>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <Text className="muted">{t("noCompletedThisWeek")}</Text>
        )}
      </section>
      <section className="content-card">
        <div className="section-title-row">
          <div>
            <Title2 as="h2">{t("reports")}</Title2>
            <Text className="muted">{t("reportsHint")}</Text>
          </div>
          <div className="inline-actions">
            {(["evening", "weekly"] as const).map((kind) => (
              <Button
                key={kind}
                disabled={digestBusy !== null}
                onClick={() => {
                  setDigestBusy(kind);
                  void supervisionApi
                    .generateDigest(kind, today)
                    .then(loadDigests)
                    .catch(reportError)
                    .finally(() => setDigestBusy(null));
                }}
              >
                {digestBusy === kind ? t("generatingReport") : t(`generateReport.${kind}`)}
              </Button>
            ))}
          </div>
        </div>
        {digests.length ? (
          <div className="digest-list">
            {digests.map((digest) => (
              <DigestCard digest={digest} t={t} key={digest.id} />
            ))}
          </div>
        ) : (
          <Text className="muted">{t("noReports")}</Text>
        )}
      </section>
    </div>
  );
}

function LocalDigestCard({
  date,
  reportError,
}: {
  date: string;
  reportError: (error: unknown) => void;
}) {
  const { t } = useTranslation();
  const [digest, setDigest] = useState<DigestItem | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(
    () =>
      supervisionApi
        .digests(20)
        .then((items) =>
          setDigest(
            items.find((item) => item.digestType === "morning" && item.periodStart === date) ??
              null,
          ),
        ),
    [date],
  );
  useEffect(() => void load().catch(() => undefined), [load]);
  return (
    <section className="content-card brief-card">
      <div className="section-title-row compact">
        <div>
          <Title2 as="h2">{t("localMorningBrief")}</Title2>
          <Text className="muted">{t("localMorningBriefHint")}</Text>
        </div>
        <Button
          appearance="primary"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            void supervisionApi
              .generateDigest("morning", date)
              .then(setDigest)
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          {busy ? t("generatingReport") : digest ? t("refreshBrief") : t("generateLocalBrief")}
        </Button>
      </div>
      {digest ? (
        <DigestCard digest={digest} t={t} compact />
      ) : (
        <Text className="muted">{t("briefNotGenerated")}</Text>
      )}
    </section>
  );
}

function DigestCard({
  digest,
  t,
  compact = false,
}: {
  digest: DigestItem;
  t: Translate;
  compact?: boolean;
}) {
  const priorities = digest.content.priorities ?? digest.content.needsReview ?? [];
  const statistics = digest.content.statistics;
  return (
    <article className={compact ? "digest-card compact" : "digest-card"}>
      <div className="section-title-row compact">
        <div>
          <strong>{digest.content.headline ?? t(`digestType.${digest.digestType}`)}</strong>
          <Text size={200} className="muted">
            {digest.periodStart}
            {digest.periodEnd !== digest.periodStart ? ` — ${digest.periodEnd}` : ""} ·{" "}
            {digest.generator}
          </Text>
        </div>
        <Badge appearance="outline">{t(`digestType.${digest.digestType}`)}</Badge>
      </div>
      {statistics && (
        <Text size={200}>
          {t("digestStatistics", {
            planned: statistics.planned ?? 0,
            completed: statistics.completed ?? 0,
            blocked: statistics.blocked ?? 0,
            canceled: statistics.canceled ?? 0,
          })}
        </Text>
      )}
      {priorities.length > 0 && (
        <ol className="digest-priorities">
          {priorities.slice(0, compact ? 3 : 8).map((item, index) => (
            <li key={`${digest.id}-${index}`}>
              <strong>{String(item.title ?? "")}</strong>
              {item.firstStep ? <span>{String(item.firstStep)}</span> : null}
            </li>
          ))}
        </ol>
      )}
    </article>
  );
}

function TaskRow({
  task,
  onToggle,
  onEdit,
  onDelete,
  onPartial,
  onCopy,
  onStatus,
  t,
  locale,
  clockFormat,
}: {
  task: TaskItem;
  onToggle: (task: TaskItem) => void;
  onEdit: (task: TaskItem) => void;
  onDelete: (task: TaskItem) => void;
  onPartial: (task: TaskItem) => void;
  onCopy: (task: TaskItem) => void;
  onStatus: (task: TaskItem, status: TaskStatus) => void;
  t: Translate;
  locale: string;
  clockFormat: Preferences["clockFormat"];
}) {
  const time = displayTaskTime(task, clockFormat, locale);
  return (
    <div className={`task-row ${task.status}`}>
      <Checkbox
        checked={task.status === "completed"}
        onChange={() => onToggle(task)}
        aria-label={t("toggleTask", { title: task.title })}
      />
      <div className="task-main">
        <div className="task-title-line">
          <Text weight="semibold">{task.title}</Text>
          {task.seriesId && (
            <Badge size="small" appearance="outline">
              {t("repeats")}
            </Badge>
          )}
          {task.isVirtual && <span className="virtual-dot" title={t("virtualOccurrence")} />}
        </div>
        {(task.notes ||
          task.completionCriteria ||
          task.dstAdjusted ||
          task.blockedReason ||
          task.snoozedUntilUtc) && (
          <Text size={200} className="muted">
            {task.blockedReason
              ? `${t("taskStatusValue.blocked")}: ${task.blockedReason}`
              : task.snoozedUntilUtc
                ? `${t("taskStatusValue.snoozed")}: ${new Date(task.snoozedUntilUtc).toLocaleString(locale)}`
                : task.dstAdjusted
                  ? t("dstAdjusted")
                  : task.completionCriteria
                    ? `${t("completionCriteria")}: ${task.completionCriteria}`
                    : task.notes}
          </Text>
        )}
      </div>
      <div className="task-meta">
        <span className={`priority-pill ${task.priority}`}>{task.priority.toUpperCase()}</span>
        {time ? <Text>{time}</Text> : <Text className="muted">{t("allDay")}</Text>}
        {task.estimatedMinutes && (
          <Text size={200} className="muted">
            {task.estimatedMinutes} min
          </Text>
        )}
        <select
          className="task-status-select"
          value={task.status}
          aria-label={t("taskStatus")}
          onChange={(event) => onStatus(task, event.target.value as TaskStatus)}
        >
          {(["todo", "in_progress", "snoozed", "blocked", "completed", "canceled"] as const).map(
            (status) => (
              <option value={status} key={status}>
                {t(`taskStatusValue.${status}`)}
              </option>
            ),
          )}
        </select>
        <div className="row-actions">
          <Button
            size="small"
            appearance="subtle"
            onClick={() => onCopy(task)}
            title={t("copyTask")}
          >
            ⧉
          </Button>
          <Button
            size="small"
            appearance="subtle"
            onClick={() => onPartial(task)}
            title={t("partialProgress")}
          >
            ◔
          </Button>
          <Button
            size="small"
            appearance="subtle"
            onClick={() => onEdit(task)}
            title={t("editTask")}
          >
            ✎
          </Button>
          <Button
            size="small"
            appearance="subtle"
            onClick={() => onDelete(task)}
            title={t("deleteTask")}
          >
            ×
          </Button>
        </div>
      </div>
    </div>
  );
}

function TaskComposer({
  initialDate,
  initialTask,
  saving,
  onClose,
  onSave,
  t,
}: {
  initialDate: string;
  initialTask: TaskItem | null;
  saving: boolean;
  onClose: () => void;
  onSave: (input: CreateTaskInput, repeat: Repeat, editScope: RecurringEditScope) => void;
  t: Translate;
}) {
  const [title, setTitle] = useState(initialTask?.title ?? "");
  const [notes, setNotes] = useState(initialTask?.notes ?? "");
  const [completionCriteria, setCompletionCriteria] = useState(
    initialTask?.completionCriteria ?? "",
  );
  const [date, setDate] = useState(initialTask ? taskDate(initialTask) : initialDate);
  const [time, setTime] = useState(initialTask ? (taskTime(initialTask) ?? "") : "");
  const [priority, setPriority] = useState<Priority>(initialTask?.priority ?? "p2");
  const [minutes, setMinutes] = useState(String(initialTask?.estimatedMinutes ?? 30));
  const [repeat, setRepeat] = useState<Repeat>("none");
  const [floating, setFloating] = useState(initialTask?.timeMode === "floating");
  const [reminder, setReminder] = useState("");
  const [editScope, setEditScope] = useState<RecurringEditScope>("only_this");
  const systemTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!title.trim()) return;
    onSave(
      {
        title: title.trim(),
        notes: notes.trim() || undefined,
        date,
        time: time || undefined,
        timeMode: time ? (floating ? "floating" : "zoned") : undefined,
        tzid: time ? systemTimeZone : undefined,
        priority,
        estimatedMinutes: minutes ? Number(minutes) : undefined,
        completionCriteria: completionCriteria.trim() || undefined,
        reminderMinutesBefore: time && reminder ? Number(reminder) : undefined,
      },
      repeat,
      editScope,
    );
  }

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => event.target === event.currentTarget && onClose()}
    >
      <section
        className="composer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="composer-title"
      >
        <div className="composer-header">
          <div>
            <Text className="date-kicker">{t("captureIntent")}</Text>
            <Title2 as="h2" id="composer-title">
              {initialTask ? t("editTask") : t("createTask")}
            </Title2>
          </div>
          <Button appearance="subtle" onClick={onClose} aria-label={t("close")}>
            ×
          </Button>
        </div>
        <form onSubmit={submit} className="composer-form">
          <label>
            <span>{t("taskTitle")}</span>
            <Input
              autoFocus
              value={title}
              onChange={(_, data) => setTitle(data.value)}
              placeholder={t("taskTitlePlaceholder")}
              maxLength={200}
              required
            />
          </label>
          <label>
            <span>{t("notesOptional")}</span>
            <Textarea
              value={notes}
              onChange={(_, data) => setNotes(data.value)}
              placeholder={t("notesPlaceholder")}
              resize="vertical"
            />
          </label>
          <label>
            <span>{t("completionCriteria")}</span>
            <Input
              value={completionCriteria}
              onChange={(_, data) => setCompletionCriteria(data.value)}
              placeholder={t("completionCriteriaPlaceholder")}
              maxLength={500}
            />
          </label>
          <div className="form-grid two">
            <label>
              <span>{t("date")}</span>
              <Input
                type="date"
                value={date}
                onChange={(_, data) => setDate(data.value)}
                required
              />
            </label>
            <label>
              <span>{t("timeOptional")}</span>
              <Input type="time" value={time} onChange={(_, data) => setTime(data.value)} />
            </label>
          </div>
          <div className="form-grid three">
            <label>
              <span>{t("priority")}</span>
              <select
                value={priority}
                onChange={(event) => setPriority(event.target.value as Priority)}
              >
                <option value="p0">P0 · {t("critical")}</option>
                <option value="p1">P1 · {t("important")}</option>
                <option value="p2">P2 · {t("normal")}</option>
                <option value="p3">P3 · {t("low")}</option>
              </select>
            </label>
            <label>
              <span>{t("estimateMinutes")}</span>
              <Input
                type="number"
                min={1}
                max={1440}
                value={minutes}
                onChange={(_, data) => setMinutes(data.value)}
              />
            </label>
            <label>
              <span>{t("repeat")}</span>
              <select
                disabled={Boolean(initialTask)}
                value={repeat}
                onChange={(event) => setRepeat(event.target.value as Repeat)}
              >
                <option value="none">{t("doesNotRepeat")}</option>
                <option value="daily">{t("daily")}</option>
                <option value="weekdays">{t("weekdays")}</option>
                <option value="weekly">{t("weekly")}</option>
                <option value="monthly">{t("monthly")}</option>
              </select>
            </label>
          </div>
          {initialTask?.seriesId && (
            <label>
              <span>{t("recurringEditScope")}</span>
              <select
                value={editScope}
                onChange={(event) => setEditScope(event.target.value as RecurringEditScope)}
              >
                <option value="only_this">{t("recurringScope.only_this")}</option>
                <option value="this_and_future">{t("recurringScope.this_and_future")}</option>
                <option value="entire_series">{t("recurringScope.entire_series")}</option>
              </select>
              <Text size={200} className="muted">
                {t(`recurringScopeHint.${editScope}`)}
              </Text>
            </label>
          )}
          {time && (
            <div className="form-grid two">
              <label>
                <span>{t("reminder")}</span>
                <select value={reminder} onChange={(event) => setReminder(event.target.value)}>
                  <option value="">{t("noReminder")}</option>
                  <option value="0">{t("atStartTime")}</option>
                  <option value="5">{t("minutesBefore", { count: 5 })}</option>
                  <option value="10">{t("minutesBefore", { count: 10 })}</option>
                  <option value="30">{t("minutesBefore", { count: 30 })}</option>
                  <option value="60">{t("hourBefore")}</option>
                </select>
              </label>
              <div />
            </div>
          )}
          {time && (
            <label className="checkbox-label">
              <Checkbox
                checked={floating}
                onChange={(_, data) => setFloating(Boolean(data.checked))}
              />
              <span>
                <strong>{t("floatingTime")}</strong>
                <Text size={200} className="muted">
                  {t("floatingTimeHint")}
                </Text>
              </span>
            </label>
          )}
          <div className="composer-actions">
            <Button appearance="secondary" onClick={onClose} type="button">
              {t("cancel")}
            </Button>
            <Button appearance="primary" type="submit" disabled={saving || !title.trim()}>
              {saving ? t("saving") : initialTask ? t("saveChanges") : t("saveTask")}
            </Button>
          </div>
        </form>
      </section>
    </div>
  );
}

function EmptyState({
  title,
  body,
  action,
  onAction,
}: {
  title: string;
  body: string;
  action: string;
  onAction: () => void;
}) {
  return (
    <div className="empty-state">
      <div className="empty-check">✓</div>
      <strong>{title}</strong>
      <Text className="muted">{body}</Text>
      <Button appearance="primary" onClick={onAction}>
        {action}
      </Button>
    </div>
  );
}

function NotificationPopover({
  notifications,
  t,
  onComplete,
  onSnooze,
  onOpen,
}: {
  notifications: NotificationItem[];
  t: Translate;
  onComplete: (notification: NotificationItem) => void;
  onSnooze: (notification: NotificationItem) => void;
  onOpen: (notification: NotificationItem) => void;
}) {
  return (
    <section className="notification-popover" aria-label={t("notifications")}>
      <strong>{t("recentNotifications")}</strong>
      {notifications.length ? (
        notifications.slice(0, 8).map((notification) => (
          <div className="notification-item" key={notification.id}>
            <span aria-hidden="true">◉</span>
            <div>
              <Text weight="semibold">{notification.title}</Text>
              <Text size={200} className="muted">
                {new Date(notification.deliveredAtUtc).toLocaleString()}
              </Text>
              <div className="notification-actions">
                <Button
                  size="small"
                  disabled={notification.taskStatus === "completed"}
                  onClick={() => onComplete(notification)}
                >
                  {t("notificationComplete")}
                </Button>
                <Button
                  size="small"
                  disabled={["completed", "canceled"].includes(notification.taskStatus)}
                  onClick={() => onSnooze(notification)}
                >
                  {t("notificationSnooze")}
                </Button>
                <Button size="small" onClick={() => onOpen(notification)}>
                  {t("notificationOpen")}
                </Button>
              </div>
            </div>
          </div>
        ))
      ) : (
        <Text className="muted">{t("noNotifications")}</Text>
      )}
    </section>
  );
}

type Translate = ReturnType<typeof useTranslation>["t"];

function taskIdentity(task: TaskItem): string {
  return task.id ?? `${task.seriesId}/${task.occurrenceKey}`;
}

function localizedLongDate(value: string, locale?: string): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  }).format(parseDate(value));
}

function localizedWeekday(value: string, locale?: string): string {
  return new Intl.DateTimeFormat(locale, { weekday: "short" }).format(parseDate(value));
}

function shiftMonth(value: string, amount: number): string {
  const date = parseDate(value);
  const day = date.getDate();
  date.setDate(1);
  date.setMonth(date.getMonth() + amount);
  const lastDay = new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate();
  date.setDate(Math.min(day, lastDay));
  return formatDate(date);
}

function displayTaskTime(
  task: TaskItem,
  clockFormat: Preferences["clockFormat"],
  locale?: string,
): string | null {
  const value = taskTime(task);
  if (!value || clockFormat === "24h") return value;
  const [hour, minute] = value.split(":").map(Number);
  return new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  }).format(new Date(2000, 0, 1, hour, minute));
}
