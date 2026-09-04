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
  Switch,
  Text,
  Textarea,
  Title2,
  Title3,
} from "@fluentui/react-components";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { FormEvent, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  BackupInfo,
  AgentBudgetSettings,
  AutomationSettings,
  BriefResult,
  KnowledgeNodeItem,
  LearningGoalItem,
  LearningResourceItem,
  LessonPlan,
  LogItem,
  NewsItem,
  Preferences,
  QuizItemView,
  QuizResult,
  ProposalPreview,
  RuntimeInfo,
  RuntimeProbe,
  TrashItem,
  WatchFieldItem,
  WatchSourceItem,
  dataApi,
  intelligenceApi,
  learningApi,
  notificationApi,
  runtimeApi,
  supervisionApi,
  taskApi,
} from "./api";

type Translate = ReturnType<typeof useTranslation>["t"];

export function IntelligenceView({ reportError }: { reportError: (error: unknown) => void }) {
  const { t, i18n } = useTranslation();
  const [fields, setFields] = useState<WatchFieldItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [sources, setSources] = useState<WatchSourceItem[]>([]);
  const [news, setNews] = useState<NewsItem[]>([]);
  const [busy, setBusy] = useState(false);
  const [fieldForm, setFieldForm] = useState(false);
  const [sourceForm, setSourceForm] = useState(false);
  const [newsFilter, setNewsFilter] = useState("latest");
  const [showGuide, setShowGuide] = useState(
    () => localStorage.getItem("pga-supervisor-onboarding") !== "done",
  );

  const load = useCallback(async () => {
    const loaded = await intelligenceApi.fields();
    setFields(loaded);
    const active =
      selectedId && loaded.some((field) => field.id === selectedId)
        ? selectedId
        : (loaded[0]?.id ?? null);
    setSelectedId(active);
    const [loadedSources, loadedNews] = await Promise.all([
      active ? intelligenceApi.sources(active) : Promise.resolve([]),
      intelligenceApi.news(active ?? undefined, newsFilter),
    ]);
    setSources(loadedSources);
    setNews(loadedNews);
  }, [newsFilter, selectedId]);

  useEffect(() => {
    void load().catch(reportError);
  }, [load, reportError]);

  async function selectField(id: string) {
    setSelectedId(id);
    try {
      const [loadedSources, loadedNews] = await Promise.all([
        intelligenceApi.sources(id),
        intelligenceApi.news(id, newsFilter),
      ]);
      setSources(loadedSources);
      setNews(loadedNews);
    } catch (error) {
      reportError(error);
    }
  }

  async function refresh() {
    setBusy(true);
    try {
      await intelligenceApi.refresh(selectedId ?? undefined);
      await load();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }

  async function updateNews(item: NewsItem, action: "read" | "saved" | "not_interested") {
    try {
      const value = action === "read" ? !item.isRead : action === "saved" ? !item.isSaved : true;
      await intelligenceApi.feedback(item.id, action, value);
      await load();
    } catch (error) {
      reportError(error);
    }
  }

  async function moveField(delta: number) {
    if (!selectedId) return;
    const ids = moveId(
      fields.map((field) => field.id),
      selectedId,
      delta,
    );
    await intelligenceApi.reorderFields(ids);
    await load();
  }

  async function moveSource(sourceId: string, delta: number) {
    if (!selectedId) return;
    const ids = moveId(
      sources.map((source) => source.id),
      sourceId,
      delta,
    );
    await intelligenceApi.reorderSources(selectedId, ids);
    await load();
  }

  const selected = fields.find((field) => field.id === selectedId);
  return (
    <div className="module-layout">
      <aside className="module-sidebar content-card">
        <div className="section-title-row compact">
          <Title2 as="h2">{t("watchFields")}</Title2>
          <Button size="small" onClick={() => setFieldForm(true)}>
            ＋
          </Button>
        </div>
        {fields.length ? (
          fields.map((field) => (
            <button
              type="button"
              className={`field-button ${selectedId === field.id ? "active" : ""}`}
              key={field.id}
              onClick={() => void selectField(field.id)}
            >
              <span>
                <strong>{field.name}</strong>
                <small>{t("sourceCount", { count: field.sourceCount })}</small>
              </span>
              {!field.enabled && <Badge size="small">{t("paused")}</Badge>}
            </button>
          ))
        ) : (
          <Text className="muted">{t("noWatchFields")}</Text>
        )}
      </aside>

      <div className="page-stack">
        {showGuide && (
          <MessageBar intent="info">
            <MessageBarBody>
              {t("supervisorGuide")}
              <div className="inline-actions guide-actions">
                <Button size="small" appearance="primary" onClick={() => setFieldForm(true)}>
                  {t("createFirstWatchField")}
                </Button>
                <Button
                  size="small"
                  onClick={() => {
                    localStorage.setItem("pga-supervisor-onboarding", "done");
                    setShowGuide(false);
                  }}
                >
                  {t("skipForNow")}
                </Button>
              </div>
            </MessageBarBody>
          </MessageBar>
        )}
        <section className="content-card">
          <div className="section-title-row">
            <div>
              <Title2 as="h2">{selected?.name ?? t("industrySignal")}</Title2>
              <Text className="muted">{selected?.description || t("industryEmptyHint")}</Text>
            </div>
            <div className="inline-actions">
              {selected && (
                <>
                  <Button
                    size="small"
                    disabled={fields[0]?.id === selected.id}
                    onClick={() => void moveField(-1).catch(reportError)}
                    title={t("moveUp")}
                  >
                    ↑
                  </Button>
                  <Button
                    size="small"
                    disabled={fields.at(-1)?.id === selected.id}
                    onClick={() => void moveField(1).catch(reportError)}
                    title={t("moveDown")}
                  >
                    ↓
                  </Button>
                  <Switch
                    checked={selected.enabled}
                    onChange={(_, data) =>
                      void intelligenceApi
                        .enableField(selected.id, data.checked)
                        .then(load)
                        .catch(reportError)
                    }
                    aria-label={t("toggleField", { name: selected.name })}
                  />
                  <Button
                    size="small"
                    appearance="subtle"
                    onClick={() => {
                      if (window.confirm(t("deleteFieldConfirm", { name: selected.name }))) {
                        void intelligenceApi.deleteField(selected.id).then(load).catch(reportError);
                      }
                    }}
                  >
                    {t("delete")}
                  </Button>
                </>
              )}
              {selected && <Button onClick={() => setSourceForm(true)}>{t("addSource")}</Button>}
              <Button
                appearance="primary"
                disabled={busy || !selected}
                onClick={() => void refresh()}
              >
                {busy ? t("refreshing") : t("refreshNow")}
              </Button>
            </div>
          </div>
          {selected && (
            <div className="source-strip">
              {sources.map((source) => (
                <div className="source-pill" key={source.id} title={source.lastError ?? source.url}>
                  <span className={source.lastError ? "status-dot error" : "status-dot"} />
                  <span>{source.name}</span>
                  <Badge size="small" appearance="outline">
                    {source.sourceType.toUpperCase()}
                  </Badge>
                  <Switch
                    checked={source.enabled}
                    onChange={(_, data) =>
                      void intelligenceApi
                        .enableSource(source.id, data.checked)
                        .then(load)
                        .catch(reportError)
                    }
                    aria-label={t("toggleSource", { name: source.name })}
                  />
                  <button
                    type="button"
                    disabled={sources[0]?.id === source.id}
                    onClick={() => void moveSource(source.id, -1).catch(reportError)}
                    title={t("moveUp")}
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    disabled={sources.at(-1)?.id === source.id}
                    onClick={() => void moveSource(source.id, 1).catch(reportError)}
                    title={t("moveDown")}
                  >
                    ↓
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      void intelligenceApi.deleteSource(source.id).then(load).catch(reportError)
                    }
                  >
                    ×
                  </button>
                </div>
              ))}
              {!sources.length && <Text className="muted">{t("addTrustedSourceHint")}</Text>}
            </div>
          )}
        </section>

        <section className="content-card">
          <div className="section-title-row compact">
            <Title2 as="h2">{t("latestIntelligence")}</Title2>
            <select value={newsFilter} onChange={(event) => setNewsFilter(event.target.value)}>
              <option value="latest">{t("latest")}</option>
              <option value="unread">{t("unread")}</option>
              <option value="saved">{t("saved")}</option>
            </select>
          </div>
          {news.length ? (
            <div className="news-list">
              {news.map((item) => (
                <article className={`news-card ${item.isRead ? "read" : ""}`} key={item.id}>
                  <div className="news-meta">
                    <Badge appearance="tint">{t(`infoKind.${item.informationKind}`)}</Badge>
                    <span>{item.sourceName}</span>
                    <span>
                      {formatTimestamp(
                        item.publishedAtUtc,
                        i18n.resolvedLanguage,
                        t("dateUnknown"),
                      )}
                    </span>
                    <span>{Math.round(item.score * 100)}%</span>
                  </div>
                  <Title3 as="h3">{item.title}</Title3>
                  <Text>{item.summary || t("sourceProvidesNoSummary")}</Text>
                  {item.uncertainty && (
                    <Text size={200} className="warning-text">
                      {item.uncertainty}
                    </Text>
                  )}
                  <div className="news-actions">
                    <Button
                      size="small"
                      appearance="primary"
                      onClick={() =>
                        void openUrl(item.canonicalUrl)
                          .then(() => updateNews(item, "read"))
                          .catch(reportError)
                      }
                    >
                      {t("openSource")}
                    </Button>
                    <Button size="small" onClick={() => void updateNews(item, "saved")}>
                      {item.isSaved ? t("unsave") : t("bookmark")}
                    </Button>
                    <Button
                      size="small"
                      appearance="subtle"
                      onClick={() => void updateNews(item, "not_interested")}
                    >
                      {t("notInterested")}
                    </Button>
                  </div>
                </article>
              ))}
            </div>
          ) : (
            <div className="quiet-empty">
              <span>◎</span>
              <strong>{t("noImportantUpdates")}</strong>
              <Text className="muted">{t("noImportantUpdatesHint")}</Text>
            </div>
          )}
        </section>
      </div>

      {fieldForm && (
        <FieldForm
          t={t}
          onClose={() => setFieldForm(false)}
          onSave={async (input) => {
            await intelligenceApi.createField(input);
            setFieldForm(false);
            await load();
          }}
          reportError={reportError}
        />
      )}
      {sourceForm && selected && (
        <SourceForm
          fieldId={selected.id}
          t={t}
          onClose={() => setSourceForm(false)}
          onSave={async (input) => {
            await intelligenceApi.createSource(input);
            setSourceForm(false);
            await load();
          }}
          reportError={reportError}
        />
      )}
    </div>
  );
}

function FieldForm({
  t,
  onClose,
  onSave,
  reportError,
}: {
  t: Translate;
  onClose: () => void;
  onSave: (input: {
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
  }) => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [include, setInclude] = useState("");
  const [exclude, setExclude] = useState("");
  const [regions, setRegions] = useState("");
  const [languages, setLanguages] = useState("zh-CN, en");
  const [maxItems, setMaxItems] = useState("5");
  const [readingMinutes, setReadingMinutes] = useState("10");
  const [weights, setWeights] = useState({
    relevance: "0.35",
    authority: "0.25",
    recency: "0.20",
    heat: "0.10",
  });
  const [breakingAlerts, setBreakingAlerts] = useState(false);
  const [saving, setSaving] = useState(false);
  function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    void onSave({
      name,
      description,
      includeTerms: splitTerms(include),
      excludeTerms: splitTerms(exclude),
      regions: splitTerms(regions),
      languages: splitTerms(languages),
      maxItems: Number(maxItems),
      readingMinutes: Number(readingMinutes),
      relevanceWeight: Number(weights.relevance),
      authorityWeight: Number(weights.authority),
      recencyWeight: Number(weights.recency),
      heatWeight: Number(weights.heat),
      breakingAlerts,
    })
      .catch(reportError)
      .finally(() => setSaving(false));
  }
  return (
    <Modal title={t("newWatchField")} onClose={onClose}>
      <form className="composer-form" onSubmit={submit}>
        <label>
          <span>{t("name")}</span>
          <Input value={name} onChange={(_, d) => setName(d.value)} required />
        </label>
        <div className="form-grid two">
          <label>
            <span>{t("regions")}</span>
            <Input
              value={regions}
              onChange={(_, data) => setRegions(data.value)}
              placeholder={t("commaSeparated")}
            />
          </label>
          <label>
            <span>{t("languages")}</span>
            <Input
              value={languages}
              onChange={(_, data) => setLanguages(data.value)}
              placeholder="zh-CN, en"
            />
          </label>
        </div>
        <div className="form-grid two">
          <label>
            <span>{t("maxItemsPerDigest")}</span>
            <Input
              type="number"
              min={1}
              max={20}
              value={maxItems}
              onChange={(_, data) => setMaxItems(data.value)}
            />
          </label>
          <label>
            <span>{t("readingMinutesLimit")}</span>
            <Input
              type="number"
              min={1}
              max={120}
              value={readingMinutes}
              onChange={(_, data) => setReadingMinutes(data.value)}
            />
          </label>
        </div>
        <div className="form-grid four">
          {(["relevance", "authority", "recency", "heat"] as const).map((weight) => (
            <label key={weight}>
              <span>{t(`weight.${weight}`)}</span>
              <Input
                type="number"
                min={weight === "authority" ? 0.01 : 0}
                max={1}
                step={0.05}
                value={weights[weight]}
                onChange={(_, data) =>
                  setWeights((current) => ({ ...current, [weight]: data.value }))
                }
              />
            </label>
          ))}
        </div>
        <label className="checkbox-label">
          <Switch
            checked={breakingAlerts}
            onChange={(_, data) => setBreakingAlerts(data.checked)}
          />
          <span>
            <strong>{t("breakingAlerts")}</strong>
            <Text size={200} className="muted">
              {t("breakingAlertsHint")}
            </Text>
          </span>
        </label>
        <MessageBar intent="info">
          <MessageBarBody>
            {t("searchPreview", {
              query:
                [...splitTerms(include), name.trim()].filter(Boolean).join(" OR ") ||
                t("searchPreviewEmpty"),
            })}
          </MessageBarBody>
        </MessageBar>
        <label>
          <span>{t("description")}</span>
          <Textarea value={description} onChange={(_, d) => setDescription(d.value)} />
        </label>
        <label>
          <span>{t("includeTerms")}</span>
          <Input
            value={include}
            onChange={(_, d) => setInclude(d.value)}
            placeholder={t("commaSeparated")}
          />
        </label>
        <label>
          <span>{t("excludeTerms")}</span>
          <Input
            value={exclude}
            onChange={(_, d) => setExclude(d.value)}
            placeholder={t("commaSeparated")}
          />
        </label>
        <FormActions t={t} onClose={onClose} saving={saving} />
      </form>
    </Modal>
  );
}

function SourceForm({
  fieldId,
  t,
  onClose,
  onSave,
  reportError,
}: {
  fieldId: string;
  t: Translate;
  onClose: () => void;
  onSave: (input: {
    fieldId: string;
    name: string;
    url: string;
    sourceType?: "rss" | "atom" | "api" | "page";
    selector?: string;
    authority?: number;
  }) => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [sourceType, setSourceType] = useState<"rss" | "atom" | "api" | "page">("rss");
  const [selector, setSelector] = useState("");
  const [saving, setSaving] = useState(false);
  function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    void onSave({ fieldId, name, url, sourceType, selector: selector || undefined })
      .catch(reportError)
      .finally(() => setSaving(false));
  }
  return (
    <Modal title={t("addTrustedSource")} onClose={onClose}>
      <form className="composer-form" onSubmit={submit}>
        <MessageBar intent="info">
          <MessageBarBody>{t("httpsSourceOnly")}</MessageBarBody>
        </MessageBar>
        <label>
          <span>{t("sourceName")}</span>
          <Input value={name} onChange={(_, d) => setName(d.value)} required />
        </label>
        <label>
          <span>{t("sourceType")}</span>
          <select
            value={sourceType}
            onChange={(event) =>
              setSourceType(event.target.value as "rss" | "atom" | "api" | "page")
            }
          >
            <option value="rss">RSS</option>
            <option value="atom">Atom</option>
            <option value="api">JSON API</option>
            <option value="page">{t("pageMonitor")}</option>
          </select>
        </label>
        <label>
          <span>{t("sourceUrl")}</span>
          <Input
            value={url}
            type="url"
            onChange={(_, d) => setUrl(d.value)}
            placeholder="https://example.com/feed.xml"
            required
          />
        </label>
        {sourceType === "page" && (
          <label>
            <span>{t("pageSelector")}</span>
            <Input
              value={selector}
              onChange={(_, data) => setSelector(data.value)}
              placeholder="main / #news / .release-notes"
            />
            <Text size={200} className="muted">
              {t("pageSelectorHint")}
            </Text>
          </label>
        )}
        <FormActions t={t} onClose={onClose} saving={saving} />
      </form>
    </Modal>
  );
}

export function LearningView({ reportError }: { reportError: (error: unknown) => void }) {
  const { t, i18n } = useTranslation();
  const today = localDateString(new Date());
  const [goals, setGoals] = useState<LearningGoalItem[]>([]);
  const [selectedGoal, setSelectedGoal] = useState<string | null>(null);
  const [nodes, setNodes] = useState<KnowledgeNodeItem[]>([]);
  const [quizzes, setQuizzes] = useState<QuizItemView[]>([]);
  const [goalForm, setGoalForm] = useState(false);
  const [nodeForm, setNodeForm] = useState(false);
  const [quizNode, setQuizNode] = useState<KnowledgeNodeItem | null>(null);
  const [activeQuiz, setActiveQuiz] = useState<QuizItemView | null>(null);
  const [quizResult, setQuizResult] = useState<QuizResult | null>(null);
  const [resourceNode, setResourceNode] = useState<KnowledgeNodeItem | null>(null);
  const [resources, setResources] = useState<LearningResourceItem[]>([]);
  const [resourceForm, setResourceForm] = useState(false);
  const [lessonPlans, setLessonPlans] = useState<LessonPlan[]>([]);
  const [lessonBusy, setLessonBusy] = useState(false);
  const [proposal, setProposal] = useState<ProposalPreview | null>(null);
  const [showTutorGuide, setShowTutorGuide] = useState(
    () => localStorage.getItem("pga-tutor-onboarding") !== "done",
  );

  const load = useCallback(async () => {
    const loadedGoals = await learningApi.goals();
    setGoals(loadedGoals);
    const goalId =
      selectedGoal && loadedGoals.some((goal) => goal.id === selectedGoal)
        ? selectedGoal
        : (loadedGoals[0]?.id ?? null);
    setSelectedGoal(goalId);
    const [loadedNodes, loadedQuizzes, loadedLessons] = await Promise.all([
      goalId ? learningApi.nodes(goalId) : Promise.resolve([]),
      learningApi.quizzes(goalId ?? undefined, false),
      supervisionApi.lessons(today),
    ]);
    setNodes(loadedNodes);
    setQuizzes(loadedQuizzes);
    setLessonPlans(loadedLessons);
  }, [selectedGoal, today]);

  useEffect(() => {
    void load().catch(reportError);
  }, [load, reportError]);
  const goal = goals.find((item) => item.id === selectedGoal);
  const due = quizzes.filter((quiz) => quiz.isDue);
  return (
    <div className="page-stack">
      {showTutorGuide && (
        <MessageBar intent="info">
          <MessageBarBody>
            {t("tutorGuide")}
            <div className="inline-actions guide-actions">
              <Button size="small" appearance="primary" onClick={() => setGoalForm(true)}>
                {t("createLearningGoal")}
              </Button>
              <Button
                size="small"
                onClick={() => {
                  localStorage.setItem("pga-tutor-onboarding", "done");
                  setShowTutorGuide(false);
                }}
              >
                {t("skipForNow")}
              </Button>
            </div>
          </MessageBarBody>
        </MessageBar>
      )}
      <section className="summary-grid">
        <Card className="summary-card accent-card">
          <Text className="summary-label">{t("activeLearningGoal")}</Text>
          <div className="summary-value compact-value">{goal?.name ?? "—"}</div>
          <Text>{goal?.purpose || t("createGoalToBegin")}</Text>
        </Card>
        <Card className="summary-card">
          <Text className="summary-label">{t("knowledgeNodes")}</Text>
          <div className="summary-value">{nodes.length}</div>
          <Text className="muted">{t("evidenceBasedProgress")}</Text>
        </Card>
        <Card className="summary-card">
          <Text className="summary-label">{t("dueReviews")}</Text>
          <div className="summary-value">{due.length}</div>
          <Text className="muted">{t("scheduledByFsrs")}</Text>
        </Card>
      </section>
      <section className="content-card">
        <div className="section-title-row">
          <div>
            <Title2 as="h2">{t("dailyLesson")}</Title2>
            <Text className="muted">{t("dailyLessonHint")}</Text>
          </div>
          <Button
            appearance="primary"
            disabled={lessonBusy || !goals.length}
            onClick={() => {
              setLessonBusy(true);
              void supervisionApi
                .generateLessons(today)
                .then(setLessonPlans)
                .catch(reportError)
                .finally(() => setLessonBusy(false));
            }}
          >
            {lessonBusy ? t("generatingLesson") : t("buildTodayLesson")}
          </Button>
        </div>
        {lessonPlans.length ? (
          <div className="lesson-plan-grid">
            {lessonPlans.map((plan) => (
              <Card key={plan.id} className="lesson-plan-card">
                <div className="section-title-row compact">
                  <div>
                    <Title3 as="h3">{plan.goalName}</Title3>
                    <Text size={200} className="muted">
                      {t("lessonBudgetSummary", {
                        planned: plan.plannedMinutes,
                        budget: plan.budgetMinutes,
                      })}
                      {` · ${t(`taskLoad.${plan.taskLoad}`)}`}
                    </Text>
                  </div>
                  <Badge appearance="tint">{t(`lessonStatus.${plan.status}`)}</Badge>
                </div>
                {plan.items.length ? (
                  <ol className="lesson-items">
                    {plan.items.map((item) => (
                      <li key={item.id}>
                        <span>{t(`lessonKind.${item.itemKind}`)}</span>
                        <strong>{item.title}</strong>
                        <small>{t("minuteCount", { count: item.minutes })}</small>
                      </li>
                    ))}
                  </ol>
                ) : (
                  <Text className="muted">{t("lessonNeedsPath")}</Text>
                )}
                <Button
                  disabled={!plan.items.length || plan.status === "approved"}
                  onClick={() =>
                    void supervisionApi
                      .proposeLessonTasks(plan.id)
                      .then(setProposal)
                      .catch(reportError)
                  }
                >
                  {t("previewLessonTasks")}
                </Button>
              </Card>
            ))}
          </div>
        ) : (
          <Text className="muted">
            {goals.length ? t("lessonNotGenerated") : t("createGoalToBegin")}
          </Text>
        )}
      </section>
      <section className="content-card">
        <div className="section-title-row">
          <div>
            <Title2 as="h2">{t("learningPath")}</Title2>
            <Text className="muted">{t("learningPathHint")}</Text>
          </div>
          <div className="inline-actions">
            <select
              value={selectedGoal ?? ""}
              onChange={(event) => setSelectedGoal(event.target.value || null)}
            >
              <option value="">{t("selectGoal")}</option>
              {goals.map((item) => (
                <option value={item.id} key={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
            <Button onClick={() => setGoalForm(true)}>＋ {t("goal")}</Button>
            <Button appearance="primary" disabled={!selectedGoal} onClick={() => setNodeForm(true)}>
              ＋ {t("knowledgeNode")}
            </Button>
          </div>
        </div>
        {nodes.length ? (
          <div className="node-grid">
            {nodes.map((node) => (
              <article className="node-card" key={node.id}>
                <div className="node-title">
                  <strong>{node.name}</strong>
                  <Badge appearance="tint">{t(`nodeStatus.${node.status}`)}</Badge>
                </div>
                <Text size={200} className="muted">
                  {node.plainExplanation || t("noExplanation")}
                </Text>
                <div className="node-outcome">
                  <Text size={200} weight="semibold">
                    {t("stageOutcome")}
                  </Text>
                  <Text size={200}>{node.stageOutcome}</Text>
                </div>
                <Text size={200} className="muted">
                  {node.estimatedMinutes
                    ? t("estimatedLearningTime", { count: node.estimatedMinutes })
                    : t("learningTimeNotEstimated")}
                  {node.prerequisiteIds.length
                    ? ` · ${t("prerequisiteSummary", {
                        names: node.prerequisiteIds
                          .map((id) => nodes.find((item) => item.id === id)?.name)
                          .filter(Boolean)
                          .join("、"),
                      })}`
                    : ` · ${t("noPrerequisites")}`}
                </Text>
                <div>
                  <Text size={200}>{t("masteryEvidence", { score: node.masteryScore })}</Text>
                  <ProgressBar value={node.masteryScore / 100} />
                </div>
                <div className="inline-actions">
                  <Button
                    size="small"
                    onClick={() => {
                      setResourceNode(node);
                      void learningApi.resources(node.id).then(setResources).catch(reportError);
                    }}
                  >
                    {t("resources")}
                  </Button>
                  <Button size="small" onClick={() => setQuizNode(node)}>
                    ＋ {t("quiz")}
                  </Button>
                  {node.dueCount > 0 && (
                    <Badge color="important">{t("dueCount", { count: node.dueCount })}</Badge>
                  )}
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div className="quiet-empty">
            <span>◇</span>
            <strong>{goals.length ? t("addFirstKnowledgeNode") : t("createLearningGoal")}</strong>
            <Text className="muted">{t("learningEmptyHint")}</Text>
          </div>
        )}
      </section>
      <section className="content-card">
        <div className="section-title-row compact">
          <Title2 as="h2">{t("reviewQueue")}</Title2>
          <Badge appearance="tint">FSRS 6.6.2</Badge>
        </div>
        {quizzes.length ? (
          <div className="quiz-list">
            {quizzes.map((quiz) => (
              <button
                className="quiz-row"
                type="button"
                key={quiz.id}
                onClick={() => {
                  setActiveQuiz(quiz);
                  setQuizResult(null);
                }}
              >
                <span>
                  <strong>{quiz.prompt}</strong>
                  <small>
                    {quiz.nodeName} · {t("difficultyLevel", { level: quiz.difficulty })}
                  </small>
                </span>
                <Badge color={quiz.isDue ? "important" : "informative"}>
                  {quiz.isDue
                    ? t("reviewNow")
                    : new Intl.DateTimeFormat(i18n.resolvedLanguage).format(new Date(quiz.dueUtc))}
                </Badge>
              </button>
            ))}
          </div>
        ) : (
          <Text className="muted">{t("noQuizYet")}</Text>
        )}
      </section>
      {goalForm && (
        <GoalForm
          t={t}
          onClose={() => setGoalForm(false)}
          reportError={reportError}
          onSave={async (input) => {
            const created = await learningApi.createGoal(input);
            setGoalForm(false);
            setSelectedGoal(created.id);
            await load();
          }}
        />
      )}
      {nodeForm && selectedGoal && (
        <NodeForm
          goalId={selectedGoal}
          existingNodes={nodes}
          t={t}
          onClose={() => setNodeForm(false)}
          reportError={reportError}
          onSave={async (input) => {
            await learningApi.createNode(input);
            setNodeForm(false);
            await load();
          }}
        />
      )}
      {quizNode && (
        <QuizForm
          node={quizNode}
          t={t}
          onClose={() => setQuizNode(null)}
          reportError={reportError}
          onSave={async (input) => {
            await learningApi.createQuiz(input);
            setQuizNode(null);
            await load();
          }}
        />
      )}
      {activeQuiz && (
        <QuizSession
          quiz={activeQuiz}
          result={quizResult}
          t={t}
          onClose={() => setActiveQuiz(null)}
          onSubmit={async (answer, confidence) => {
            const result = await learningApi.submit(activeQuiz.id, answer, confidence);
            setQuizResult(result);
            await load();
          }}
          reportError={reportError}
        />
      )}
      {resourceNode && (
        <Modal
          title={t("resourcesForNode", { name: resourceNode.name })}
          onClose={() => {
            setResourceNode(null);
            setResourceForm(false);
          }}
        >
          <div className="resource-modal">
            <div className="section-title-row compact">
              <Text className="muted">{t("resourceMetadataHint")}</Text>
              <Button appearance="primary" size="small" onClick={() => setResourceForm(true)}>
                ＋ {t("resource")}
              </Button>
            </div>
            {resources.length ? (
              <div className="resource-list">
                {resources.map((resource) => (
                  <article className="resource-card" key={resource.id}>
                    <div>
                      <strong>{resource.title}</strong>
                      <Text size={200} className="muted">
                        {[resource.author, resource.publishedDate || t("dateUnknown")]
                          .filter(Boolean)
                          .join(" · ")}
                      </Text>
                    </div>
                    <div className="news-meta">
                      <Badge appearance="tint">{resource.resourceType}</Badge>
                      {resource.difficulty && <span>{resource.difficulty}</span>}
                      {resource.durationMinutes && (
                        <span>{t("minuteCount", { count: resource.durationMinutes })}</span>
                      )}
                      {resource.cost && <span>{resource.cost}</span>}
                      {resource.versionFit && <span>{resource.versionFit}</span>}
                    </div>
                    {resource.recommendationReason && <Text>{resource.recommendationReason}</Text>}
                    <Button
                      size="small"
                      onClick={() => void openUrl(resource.url).catch(reportError)}
                    >
                      {t("openResource")}
                    </Button>
                  </article>
                ))}
              </div>
            ) : (
              <Text className="muted">{t("noResourcesYet")}</Text>
            )}
          </div>
          {resourceForm && (
            <ResourceForm
              nodeId={resourceNode.id}
              t={t}
              onClose={() => setResourceForm(false)}
              reportError={reportError}
              onSave={async (input) => {
                await learningApi.createResource(input);
                setResources(await learningApi.resources(resourceNode.id));
                setResourceForm(false);
              }}
            />
          )}
        </Modal>
      )}
      {proposal && (
        <Modal
          title={t("proposalPreview")}
          onClose={() => {
            void supervisionApi.rejectProposal(proposal.id).catch(() => undefined);
            setProposal(null);
          }}
        >
          <MessageBar intent="warning">
            <MessageBarBody>{t("proposalApprovalWarning")}</MessageBarBody>
          </MessageBar>
          <div className="proposal-diff">
            {(proposal.diff.tasks ?? []).map((task, index) => (
              <div key={`${proposal.id}-${index}`}>
                <strong>＋ {String(task.title ?? "")}</strong>
                <Text size={200} className="muted">
                  {String(task.date ?? "")} · {String(task.estimatedMinutes ?? "")} min
                </Text>
              </div>
            ))}
          </div>
          <div className="composer-actions">
            <Button
              onClick={() => {
                void supervisionApi.rejectProposal(proposal.id).finally(() => setProposal(null));
              }}
            >
              {t("rejectProposal")}
            </Button>
            <Button
              appearance="primary"
              disabled={!proposal.approvalToken}
              onClick={() => {
                if (!proposal.approvalToken) return;
                void supervisionApi
                  .applyProposal(proposal.id, proposal.approvalToken)
                  .then(async () => {
                    setProposal(null);
                    window.dispatchEvent(new Event("pga-tasks-changed"));
                    await load();
                  })
                  .catch(reportError);
              }}
            >
              {t("approveAndCreateTasks")}
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}

function ResourceForm({
  nodeId,
  t,
  onClose,
  onSave,
  reportError,
}: {
  nodeId: string;
  t: Translate;
  onClose: () => void;
  onSave: (input: {
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
  }) => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [title, setTitle] = useState("");
  const [url, setUrl] = useState("");
  const [author, setAuthor] = useState("");
  const [publishedDate, setPublishedDate] = useState("");
  const [resourceType, setResourceType] = useState("article");
  const [duration, setDuration] = useState("");
  const [difficulty, setDifficulty] = useState("");
  const [cost, setCost] = useState("");
  const [versionFit, setVersionFit] = useState("");
  const [reason, setReason] = useState("");
  const [saving, setSaving] = useState(false);
  return (
    <Modal title={t("addLearningResource")} onClose={onClose}>
      <form
        className="composer-form"
        onSubmit={(event) => {
          event.preventDefault();
          setSaving(true);
          void onSave({
            nodeId,
            title,
            url,
            author: author || undefined,
            publishedDate: publishedDate || undefined,
            resourceType,
            durationMinutes: duration ? Number(duration) : undefined,
            difficulty: difficulty || undefined,
            cost: cost || undefined,
            versionFit: versionFit || undefined,
            recommendationReason: reason,
          })
            .catch(reportError)
            .finally(() => setSaving(false));
        }}
      >
        <label>
          <span>{t("resourceTitle")}</span>
          <Input value={title} onChange={(_, data) => setTitle(data.value)} required />
        </label>
        <label>
          <span>{t("resourceUrl")}</span>
          <Input type="url" value={url} onChange={(_, data) => setUrl(data.value)} required />
        </label>
        <div className="form-grid">
          <label>
            <span>{t("author")}</span>
            <Input value={author} onChange={(_, data) => setAuthor(data.value)} />
          </label>
          <label>
            <span>{t("publishedDate")}</span>
            <Input
              type="date"
              value={publishedDate}
              onChange={(_, data) => setPublishedDate(data.value)}
            />
          </label>
          <label>
            <span>{t("resourceType")}</span>
            <select value={resourceType} onChange={(event) => setResourceType(event.target.value)}>
              <option value="article">{t("resourceTypeArticle")}</option>
              <option value="video">{t("resourceTypeVideo")}</option>
              <option value="course">{t("resourceTypeCourse")}</option>
              <option value="book">{t("resourceTypeBook")}</option>
              <option value="documentation">{t("resourceTypeDocumentation")}</option>
            </select>
          </label>
          <label>
            <span>{t("durationMinutes")}</span>
            <Input
              type="number"
              min={1}
              value={duration}
              onChange={(_, data) => setDuration(data.value)}
            />
          </label>
          <label>
            <span>{t("difficulty")}</span>
            <Input value={difficulty} onChange={(_, data) => setDifficulty(data.value)} />
          </label>
          <label>
            <span>{t("cost")}</span>
            <Input value={cost} onChange={(_, data) => setCost(data.value)} />
          </label>
        </div>
        <label>
          <span>{t("versionFit")}</span>
          <Input value={versionFit} onChange={(_, data) => setVersionFit(data.value)} />
        </label>
        <label>
          <span>{t("recommendationReason")}</span>
          <Textarea value={reason} onChange={(_, data) => setReason(data.value)} />
        </label>
        <FormActions t={t} onClose={onClose} saving={saving} />
      </form>
    </Modal>
  );
}

function GoalForm({
  t,
  onClose,
  onSave,
  reportError,
}: {
  t: Translate;
  onClose: () => void;
  onSave: (input: {
    name: string;
    purpose: string;
    currentLevel: string;
    targetLevel: string;
    targetDate?: string;
    dailyMinutes?: number;
    weeklyMinutes?: number;
    language?: string;
    resourcePreferences?: string[];
    budgetMode?: "free_first" | "paid_allowed";
    autoAddLessons?: boolean;
  }) => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [name, setName] = useState("");
  const [purpose, setPurpose] = useState("");
  const [daily, setDaily] = useState("30");
  const [weekly, setWeekly] = useState("180");
  const [currentLevel, setCurrentLevel] = useState("beginner");
  const [targetLevel, setTargetLevel] = useState("competent");
  const [targetDate, setTargetDate] = useState("");
  const [language, setLanguage] = useState("zh-CN");
  const [resourcePreferences, setResourcePreferences] = useState("article, interactive");
  const [budgetMode, setBudgetMode] = useState<"free_first" | "paid_allowed">("free_first");
  const [autoAddLessons, setAutoAddLessons] = useState(false);
  const [saving, setSaving] = useState(false);
  function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    void onSave({
      name,
      purpose,
      currentLevel,
      targetLevel,
      targetDate: targetDate || undefined,
      dailyMinutes: Number(daily),
      weeklyMinutes: Number(weekly),
      language,
      resourcePreferences: splitTerms(resourcePreferences),
      budgetMode,
      autoAddLessons,
    })
      .catch(reportError)
      .finally(() => setSaving(false));
  }
  return (
    <Modal title={t("createLearningGoal")} onClose={onClose}>
      <form className="composer-form" onSubmit={submit}>
        <label>
          <span>{t("goalName")}</span>
          <Input value={name} onChange={(_, d) => setName(d.value)} required />
        </label>
        <label>
          <span>{t("learningPurpose")}</span>
          <Textarea value={purpose} onChange={(_, d) => setPurpose(d.value)} />
        </label>
        <div className="form-grid two">
          <label>
            <span>{t("currentLevel")}</span>
            <Input value={currentLevel} onChange={(_, d) => setCurrentLevel(d.value)} required />
          </label>
          <label>
            <span>{t("targetLevel")}</span>
            <Input value={targetLevel} onChange={(_, d) => setTargetLevel(d.value)} required />
          </label>
          <label>
            <span>{t("dailyBudgetMinutes")}</span>
            <Input
              type="number"
              min={5}
              max={1440}
              value={daily}
              onChange={(_, d) => setDaily(d.value)}
            />
          </label>
          <label>
            <span>{t("weeklyBudgetMinutes")}</span>
            <Input
              type="number"
              min={5}
              max={10080}
              value={weekly}
              onChange={(_, d) => setWeekly(d.value)}
            />
          </label>
        </div>
        <label>
          <span>{t("targetDate")}</span>
          <Input type="date" value={targetDate} onChange={(_, d) => setTargetDate(d.value)} />
        </label>
        <div className="form-grid two">
          <label>
            <span>{t("learningLanguage")}</span>
            <Input value={language} onChange={(_, data) => setLanguage(data.value)} />
          </label>
          <label>
            <span>{t("resourcePreferences")}</span>
            <Input
              value={resourcePreferences}
              onChange={(_, data) => setResourcePreferences(data.value)}
              placeholder={t("commaSeparated")}
            />
          </label>
          <label>
            <span>{t("resourceBudgetMode")}</span>
            <select
              value={budgetMode}
              onChange={(event) =>
                setBudgetMode(event.target.value as "free_first" | "paid_allowed")
              }
            >
              <option value="free_first">{t("freeFirst")}</option>
              <option value="paid_allowed">{t("paidAllowed")}</option>
            </select>
          </label>
        </div>
        <label className="checkbox-label">
          <Switch
            checked={autoAddLessons}
            onChange={(_, data) => setAutoAddLessons(data.checked)}
          />
          <span>
            <strong>{t("allowLessonTaskConversion")}</strong>
            <Text size={200} className="muted">
              {t("allowLessonTaskConversionHint")}
            </Text>
          </span>
        </label>
        <FormActions t={t} onClose={onClose} saving={saving} />
      </form>
    </Modal>
  );
}

function NodeForm({
  goalId,
  existingNodes,
  t,
  onClose,
  onSave,
  reportError,
}: {
  goalId: string;
  existingNodes: KnowledgeNodeItem[];
  t: Translate;
  onClose: () => void;
  onSave: (input: {
    goalId: string;
    name: string;
    plainExplanation: string;
    stageOutcome: string;
    estimatedMinutes?: number;
    prerequisiteIds: string[];
  }) => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [name, setName] = useState("");
  const [explanation, setExplanation] = useState("");
  const [stageOutcome, setStageOutcome] = useState("");
  const [estimatedMinutes, setEstimatedMinutes] = useState("60");
  const [prerequisiteIds, setPrerequisiteIds] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    void onSave({
      goalId,
      name,
      plainExplanation: explanation,
      stageOutcome,
      estimatedMinutes: estimatedMinutes ? Number(estimatedMinutes) : undefined,
      prerequisiteIds,
    })
      .catch(reportError)
      .finally(() => setSaving(false));
  }
  return (
    <Modal title={t("addKnowledgeNode")} onClose={onClose}>
      <form className="composer-form" onSubmit={submit}>
        <label>
          <span>{t("nodeName")}</span>
          <Input value={name} onChange={(_, d) => setName(d.value)} required />
        </label>
        <label>
          <span>{t("plainExplanation")}</span>
          <Textarea value={explanation} onChange={(_, d) => setExplanation(d.value)} />
        </label>
        <label>
          <span>{t("stageOutcome")}</span>
          <Textarea
            value={stageOutcome}
            onChange={(_, d) => setStageOutcome(d.value)}
            placeholder={t("stageOutcomePlaceholder")}
            required
          />
        </label>
        <label>
          <span>{t("estimatedLearningMinutes")}</span>
          <Input
            type="number"
            min={1}
            max={10080}
            value={estimatedMinutes}
            onChange={(_, d) => setEstimatedMinutes(d.value)}
          />
        </label>
        {existingNodes.length > 0 && (
          <fieldset className="prerequisite-fieldset">
            <legend>{t("prerequisiteKnowledge")}</legend>
            <Text size={200} className="muted">
              {t("prerequisiteHint")}
            </Text>
            {existingNodes.map((node) => (
              <label className="checkbox-label" key={node.id}>
                <Checkbox
                  checked={prerequisiteIds.includes(node.id)}
                  onChange={(_, data) =>
                    setPrerequisiteIds((current) =>
                      data.checked ? [...current, node.id] : current.filter((id) => id !== node.id),
                    )
                  }
                />
                <span>{node.name}</span>
              </label>
            ))}
          </fieldset>
        )}
        <FormActions t={t} onClose={onClose} saving={saving} />
      </form>
    </Modal>
  );
}

function QuizForm({
  node,
  t,
  onClose,
  onSave,
  reportError,
}: {
  node: KnowledgeNodeItem;
  t: Translate;
  onClose: () => void;
  onSave: (input: {
    nodeId: string;
    questionType: "single";
    prompt: string;
    options: string[];
    correctAnswer: string[];
    explanation: string;
  }) => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [prompt, setPrompt] = useState("");
  const [options, setOptions] = useState("");
  const [answer, setAnswer] = useState("");
  const [explanation, setExplanation] = useState("");
  const [saving, setSaving] = useState(false);
  function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    void onSave({
      nodeId: node.id,
      questionType: "single",
      prompt,
      options: splitTerms(options),
      correctAnswer: [answer.trim()],
      explanation,
    })
      .catch(reportError)
      .finally(() => setSaving(false));
  }
  return (
    <Modal title={`${t("newQuiz")} · ${node.name}`} onClose={onClose}>
      <form className="composer-form" onSubmit={submit}>
        <label>
          <span>{t("question")}</span>
          <Textarea value={prompt} onChange={(_, d) => setPrompt(d.value)} required />
        </label>
        <label>
          <span>{t("options")}</span>
          <Input
            value={options}
            onChange={(_, d) => setOptions(d.value)}
            placeholder={t("commaSeparated")}
            required
          />
        </label>
        <label>
          <span>{t("correctAnswer")}</span>
          <Input value={answer} onChange={(_, d) => setAnswer(d.value)} required />
        </label>
        <label>
          <span>{t("answerExplanation")}</span>
          <Textarea value={explanation} onChange={(_, d) => setExplanation(d.value)} />
        </label>
        <FormActions t={t} onClose={onClose} saving={saving} />
      </form>
    </Modal>
  );
}

function QuizSession({
  quiz,
  result,
  t,
  onClose,
  onSubmit,
  reportError,
}: {
  quiz: QuizItemView;
  result: QuizResult | null;
  t: Translate;
  onClose: () => void;
  onSubmit: (answer: string[], confidence: "sure" | "unsure" | "skipped") => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const [selected, setSelected] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  return (
    <Modal title={`${t("quiz")} · ${quiz.nodeName}`} onClose={onClose}>
      <div className="quiz-session">
        <Title3 as="h3">{quiz.prompt}</Title3>
        {quiz.options.map((option) => (
          <label className="answer-option" key={option}>
            <Checkbox
              checked={selected.includes(option)}
              disabled={Boolean(result)}
              onChange={(_, data) => setSelected(data.checked ? [option] : [])}
            />
            <span>{option}</span>
          </label>
        ))}
        {result ? (
          <MessageBar intent={result.isCorrect ? "success" : "error"}>
            <MessageBarBody>
              <strong>{result.isCorrect ? t("answerCorrect") : t("answerIncorrect")}</strong>
              <br />
              {t("correctAnswerIs", { answer: result.correctAnswer.join(", ") })}
              <br />
              {result.explanation}
              <br />
              {t("nextReviewAt", { date: new Date(result.nextDueUtc).toLocaleString() })}
            </MessageBarBody>
          </MessageBar>
        ) : (
          <div className="composer-actions">
            <Button
              onClick={() => {
                setSaving(true);
                void onSubmit([], "skipped")
                  .catch(reportError)
                  .finally(() => setSaving(false));
              }}
            >
              {t("skip")}
            </Button>
            <Button
              appearance="primary"
              disabled={!selected.length || saving}
              onClick={() => {
                setSaving(true);
                void onSubmit(selected, "sure")
                  .catch(reportError)
                  .finally(() => setSaving(false));
              }}
            >
              {t("submitAnswer")}
            </Button>
          </div>
        )}
      </div>
    </Modal>
  );
}

export function SettingsView({
  info,
  onLanguage,
  reportError,
}: {
  info: RuntimeInfo | null;
  onLanguage: () => Promise<void>;
  reportError: (error: unknown) => void;
}) {
  const { t } = useTranslation();
  const [preferences, setPreferences] = useState<Preferences>({
    theme: "system",
    weekStartsOn: "monday",
    clockFormat: "24h",
    closeToTray: true,
    notificationsEnabled: true,
    appTimezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
  });
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [trash, setTrash] = useState<TrashItem[]>([]);
  const [probe, setProbe] = useState<RuntimeProbe | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [licenseText, setLicenseText] = useState<string | null>(null);
  const [startupEnabled, setStartupEnabled] = useState(false);
  const [automation, setAutomation] = useState<AutomationSettings>({
    morningEnabled: true,
    morningTime: "08:00",
    eveningEnabled: true,
    eveningTime: "21:30",
    weeklyEnabled: true,
    weeklyWeekday: 6,
    weeklyTime: "20:30",
    intelligenceEnabled: true,
    intelligenceIntervalHours: 6,
    lessonEnabled: true,
    lessonTime: "07:45",
  });
  const [failures, setFailures] = useState<LogItem[]>([]);
  const [agentBudget, setAgentBudget] = useState<AgentBudgetSettings>({
    enabled: true,
    dailyRunLimit: 2,
    monthlyRunLimit: 40,
    budgetMode: "saving",
  });
  const load = useCallback(async () => {
    const [
      prefs,
      backupItems,
      trashItems,
      runtime,
      startsWithWindows,
      jobs,
      recentFailures,
      budget,
    ] = await Promise.all([
      dataApi.preferences(),
      dataApi.backups(),
      taskApi.listTrash(),
      runtimeApi.probe(),
      dataApi.startupEnabled(),
      supervisionApi.automationSettings(),
      supervisionApi.recentFailures(),
      runtimeApi.budget(),
    ]);
    setPreferences(prefs);
    setBackups(backupItems);
    setTrash(trashItems);
    setProbe(runtime);
    setStartupEnabled(startsWithWindows);
    setAutomation(jobs);
    setFailures(recentFailures);
    setAgentBudget(budget);
  }, []);
  useEffect(() => {
    void load().catch(reportError);
  }, [load, reportError]);
  useEffect(() => applyTheme(preferences.theme), [preferences.theme]);
  async function savePreferences(next: Preferences) {
    setPreferences(next);
    localStorage.setItem("pga-theme", next.theme);
    window.dispatchEvent(new CustomEvent("pga-theme-changed", { detail: next.theme }));
    window.dispatchEvent(new CustomEvent("pga-preferences-changed", { detail: next }));
    try {
      await dataApi.savePreferences(next);
    } catch (error) {
      reportError(error);
      await load();
    }
  }
  async function action(operation: () => Promise<unknown>, success: string) {
    setBusy(true);
    setNotice(null);
    try {
      await operation();
      setNotice(success);
      await load();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }
  async function saveAutomation() {
    setBusy(true);
    try {
      setAutomation(await supervisionApi.saveAutomationSettings(automation));
      setNotice(t("automationSaved"));
    } catch (error) {
      reportError(error);
      await load();
    } finally {
      setBusy(false);
    }
  }
  async function saveAgentBudget() {
    setBusy(true);
    try {
      setAgentBudget(await runtimeApi.saveBudget(agentBudget));
      setNotice(t("agentBudgetSaved"));
    } catch (error) {
      reportError(error);
      await load();
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="settings-grid">
      {notice && (
        <MessageBar intent="success" className="settings-span">
          <MessageBarBody>{notice}</MessageBarBody>
        </MessageBar>
      )}
      <section className="content-card">
        <Title2 as="h2">{t("languageAndDisplay")}</Title2>
        <div className="setting-list">
          <Setting label={t("interfaceLanguage")} hint={t("languageHint")}>
            <Button onClick={() => void onLanguage()}>{t("switchLanguage")}</Button>
          </Setting>
          <Setting label={t("theme")} hint={t("themeHint")}>
            <select
              value={preferences.theme}
              onChange={(event) =>
                void savePreferences({
                  ...preferences,
                  theme: event.target.value as Preferences["theme"],
                })
              }
            >
              <option value="system">{t("themeSystem")}</option>
              <option value="light">{t("themeLight")}</option>
              <option value="dark">{t("themeDark")}</option>
            </select>
          </Setting>
          <Setting label={t("weekStart")} hint={t("weekStartHint")}>
            <select
              value={preferences.weekStartsOn}
              onChange={(event) =>
                void savePreferences({
                  ...preferences,
                  weekStartsOn: event.target.value as Preferences["weekStartsOn"],
                })
              }
            >
              <option value="monday">{t("monday")}</option>
              <option value="sunday">{t("sunday")}</option>
            </select>
          </Setting>
          <Setting label={t("clockFormat")} hint={t("clockFormatHint")}>
            <select
              value={preferences.clockFormat}
              onChange={(event) =>
                void savePreferences({
                  ...preferences,
                  clockFormat: event.target.value as Preferences["clockFormat"],
                })
              }
            >
              <option value="24h">24 h</option>
              <option value="12h">12 h</option>
            </select>
          </Setting>
          <Setting label={t("appTimezone")} hint={t("appTimezoneHint")}>
            <Text>{preferences.appTimezone}</Text>
          </Setting>
          <Setting label={t("closeToTray")} hint={t("closeToTrayHint")}>
            <Switch
              checked={preferences.closeToTray}
              onChange={(_, data) =>
                void savePreferences({ ...preferences, closeToTray: data.checked })
              }
            />
          </Setting>
          <Setting label={t("startWithWindows")} hint={t("startWithWindowsHint")}>
            <Switch
              checked={startupEnabled}
              onChange={(_, data) => {
                const next = data.checked;
                setStartupEnabled(next);
                void dataApi
                  .setStartupEnabled(next)
                  .then(setStartupEnabled)
                  .catch((error) => {
                    setStartupEnabled(!next);
                    reportError(error);
                  });
              }}
            />
          </Setting>
          <Setting label={t("enableNotifications")} hint={t("enableNotificationsHint")}>
            <Switch
              checked={preferences.notificationsEnabled}
              onChange={(_, data) =>
                void savePreferences({ ...preferences, notificationsEnabled: data.checked })
              }
            />
          </Setting>
          <Setting label={t("quietControls")} hint={t("quietControlsHint")}>
            <div className="inline-actions">
              <Button
                size="small"
                onClick={() =>
                  void action(() => notificationApi.pause("one_hour"), t("notificationsPausedHour"))
                }
              >
                {t("pauseOneHour")}
              </Button>
              <Button
                size="small"
                onClick={() =>
                  void action(() => notificationApi.pause("today"), t("notificationsQuietToday"))
                }
              >
                {t("quietToday")}
              </Button>
              <Button
                size="small"
                onClick={() =>
                  void action(() => notificationApi.pause("clear"), t("notificationsResumed"))
                }
              >
                {t("resumeNotifications")}
              </Button>
            </div>
          </Setting>
        </div>
      </section>
      <section className="content-card">
        <Title2 as="h2">{t("dataAndBackup")}</Title2>
        <Text className="muted">{t("backupHint")}</Text>
        <div className="data-actions">
          <Button
            appearance="primary"
            disabled={busy}
            onClick={() => void action(() => dataApi.createBackup(), t("backupCreated"))}
          >
            {t("backupNow")}
          </Button>
          <Button
            disabled={busy}
            onClick={() =>
              void action(
                () => dataApi.exportData().then((result) => revealItemInDir(result.path)),
                t("exportCreated"),
              )
            }
          >
            {t("exportJson")}
          </Button>
        </div>
        <div className="backup-list">
          {backups.slice(0, 5).map((backup) => (
            <div key={backup.name}>
              <span>
                <strong>{backup.name}</strong>
                <small>{formatBytes(backup.sizeBytes)}</small>
              </span>
              <Button
                size="small"
                disabled={busy}
                onClick={() =>
                  void action(() => dataApi.restoreBackup(backup.name), t("restartToRestore"))
                }
              >
                {t("restore")}
              </Button>
            </div>
          ))}
        </div>
      </section>
      <section className="content-card">
        <Title2 as="h2">{t("recycleBin")}</Title2>
        <Text className="muted">{t("recycleBinHint")}</Text>
        {trash.length ? (
          <div className="backup-list">
            {trash.map((item) => (
              <div key={item.id}>
                <span>
                  <strong>{item.task.title}</strong>
                  <small>{new Date(item.deletedAtUtc).toLocaleString()}</small>
                </span>
                <Button
                  size="small"
                  onClick={() => void action(() => taskApi.restore(item.id), t("taskRestored"))}
                >
                  {t("restore")}
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <Text className="muted list-empty">{t("recycleBinEmpty")}</Text>
        )}
      </section>
      <section className="content-card">
        <Title2 as="h2">{t("agentRuntime")}</Title2>
        {probe ? (
          <>
            <div className="runtime-status">
              <span className={`status-dot ${probe.state === "ready" ? "" : "warning"}`} />
              <div>
                <strong>{t(`runtimeState.${probe.state}`)}</strong>
                <Text size={200} className="muted">
                  {probe.message}
                </Text>
              </div>
            </div>
            <Text size={200} className="path-value">
              {probe.isolatedHome}
            </Text>
            <div className="data-actions">
              {probe.state === "not_authenticated" && (
                <Button onClick={() => void action(() => runtimeApi.login(), t("loginStarted"))}>
                  {t("loginIsolatedRuntime")}
                </Button>
              )}
              {probe.state === "ready" && (
                <Button onClick={() => void action(() => runtimeApi.logout(), t("loggedOut"))}>
                  {t("logout")}
                </Button>
              )}
              <Button onClick={() => void action(() => runtimeApi.probe(), t("runtimeChecked"))}>
                {t("checkAgain")}
              </Button>
            </div>
            <div className="setting-list runtime-budget">
              <Setting label={t("enableAgentRuntime")} hint={t("enableAgentRuntimeHint")}>
                <Switch
                  checked={agentBudget.enabled}
                  onChange={(_, data) => setAgentBudget({ ...agentBudget, enabled: data.checked })}
                />
              </Setting>
              <Setting label={t("budgetMode")} hint={t("budgetModeHint")}>
                <select
                  value={agentBudget.budgetMode}
                  onChange={(event) =>
                    setAgentBudget({
                      ...agentBudget,
                      budgetMode: event.target.value as AgentBudgetSettings["budgetMode"],
                    })
                  }
                >
                  <option value="saving">{t("budgetMode.saving")}</option>
                  <option value="standard">{t("budgetMode.standard")}</option>
                  <option value="deep">{t("budgetMode.deep")}</option>
                </select>
              </Setting>
              <Setting label={t("dailyAiRunLimit")} hint={t("dailyAiRunLimitHint")}>
                <Input
                  type="number"
                  min={0}
                  max={20}
                  value={String(agentBudget.dailyRunLimit)}
                  onChange={(_, data) => {
                    const value = Number(data.value);
                    if (Number.isInteger(value) && value >= 0 && value <= 20)
                      setAgentBudget({ ...agentBudget, dailyRunLimit: value });
                  }}
                />
              </Setting>
              <Setting label={t("monthlyAiRunLimit")} hint={t("monthlyAiRunLimitHint")}>
                <Input
                  type="number"
                  min={0}
                  max={500}
                  value={String(agentBudget.monthlyRunLimit)}
                  onChange={(_, data) => {
                    const value = Number(data.value);
                    if (Number.isInteger(value) && value >= 0 && value <= 500)
                      setAgentBudget({ ...agentBudget, monthlyRunLimit: value });
                  }}
                />
              </Setting>
              <Button appearance="primary" disabled={busy} onClick={() => void saveAgentBudget()}>
                {t("saveChanges")}
              </Button>
            </div>
          </>
        ) : (
          <Spinner size="tiny" />
        )}
      </section>
      <section className="content-card settings-span">
        <div className="section-title-row">
          <div>
            <Title2 as="h2">{t("automaticSchedule")}</Title2>
            <Text className="muted">{t("automaticScheduleHint")}</Text>
          </div>
          <Button appearance="primary" disabled={busy} onClick={() => void saveAutomation()}>
            {t("saveChanges")}
          </Button>
        </div>
        <div className="automation-grid">
          <Setting label={t("morningBriefTime")} hint={t("localRulesNoAi")}>
            <div className="inline-actions">
              <Switch
                checked={automation.morningEnabled}
                onChange={(_, data) =>
                  setAutomation({ ...automation, morningEnabled: data.checked })
                }
              />
              <Input
                type="time"
                value={automation.morningTime}
                onChange={(_, data) => setAutomation({ ...automation, morningTime: data.value })}
              />
            </div>
          </Setting>
          <Setting label={t("eveningReviewTime")} hint={t("localRulesNoAi")}>
            <div className="inline-actions">
              <Switch
                checked={automation.eveningEnabled}
                onChange={(_, data) =>
                  setAutomation({ ...automation, eveningEnabled: data.checked })
                }
              />
              <Input
                type="time"
                value={automation.eveningTime}
                onChange={(_, data) => setAutomation({ ...automation, eveningTime: data.value })}
              />
            </div>
          </Setting>
          <Setting label={t("weeklyReportTime")} hint={t("weekdayZeroHint")}>
            <div className="inline-actions">
              <Switch
                checked={automation.weeklyEnabled}
                onChange={(_, data) =>
                  setAutomation({ ...automation, weeklyEnabled: data.checked })
                }
              />
              <select
                value={automation.weeklyWeekday}
                onChange={(event) =>
                  setAutomation({ ...automation, weeklyWeekday: Number(event.target.value) })
                }
              >
                {["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"].map(
                  (day, index) => (
                    <option value={index} key={day}>
                      {t(day)}
                    </option>
                  ),
                )}
              </select>
              <Input
                type="time"
                value={automation.weeklyTime}
                onChange={(_, data) => setAutomation({ ...automation, weeklyTime: data.value })}
              />
            </div>
          </Setting>
          <Setting label={t("intelligenceSchedule")} hint={t("intelligenceScheduleHint")}>
            <div className="inline-actions">
              <Switch
                checked={automation.intelligenceEnabled}
                onChange={(_, data) =>
                  setAutomation({ ...automation, intelligenceEnabled: data.checked })
                }
              />
              <Input
                type="number"
                min={1}
                max={24}
                value={String(automation.intelligenceIntervalHours)}
                onChange={(_, data) =>
                  setAutomation({
                    ...automation,
                    intelligenceIntervalHours: Number(data.value),
                  })
                }
              />
              <Text size={200}>{t("hours")}</Text>
            </div>
          </Setting>
          <Setting label={t("dailyLessonTime")} hint={t("lessonScheduleHint")}>
            <div className="inline-actions">
              <Switch
                checked={automation.lessonEnabled}
                onChange={(_, data) =>
                  setAutomation({ ...automation, lessonEnabled: data.checked })
                }
              />
              <Input
                type="time"
                value={automation.lessonTime}
                onChange={(_, data) => setAutomation({ ...automation, lessonTime: data.value })}
              />
            </div>
          </Setting>
        </div>
      </section>
      <section className="content-card settings-span">
        <div className="section-title-row compact">
          <div>
            <Title2 as="h2">{t("recentFailures")}</Title2>
            <Text className="muted">{t("recentFailuresHint")}</Text>
          </div>
          <Button
            onClick={() =>
              void supervisionApi.recentFailures().then(setFailures).catch(reportError)
            }
          >
            {t("refresh")}
          </Button>
        </div>
        {failures.length ? (
          <div className="failure-list">
            {failures.map((failure) => (
              <div key={failure.id}>
                <Badge color={failure.level === "error" ? "danger" : "warning"} appearance="tint">
                  {failure.category}
                </Badge>
                <span>
                  <strong>{failure.eventName}</strong>
                  <small>{failure.message}</small>
                </span>
                <time>{new Date(failure.createdAtUtc).toLocaleString()}</time>
              </div>
            ))}
          </div>
        ) : (
          <Text className="muted">{t("noRecentFailures")}</Text>
        )}
      </section>
      <section className="content-card">
        <Title2 as="h2">{t("localRuntime")}</Title2>
        {info ? (
          <dl className="runtime-list">
            <dt>{t("version")}</dt>
            <dd>{info.appVersion}</dd>
            <dt>{t("database")}</dt>
            <dd>{info.databaseEngine}</dd>
            <dt>{t("recurrence")}</dt>
            <dd>{info.recurrenceEngine}</dd>
            <dt>{t("timezoneData")}</dt>
            <dd>{info.timezoneDatabase}</dd>
            <dt>{t("agentBridge")}</dt>
            <dd>{info.agentBridge}</dd>
            <dt>{t("dataLocation")}</dt>
            <dd className="path-value">{info.dataDirectory}</dd>
          </dl>
        ) : (
          <Spinner size="tiny" />
        )}
      </section>
      <section className="content-card privacy-card settings-span">
        <Title2 as="h2">{t("privacyByDefault")}</Title2>
        <Text>{t("privacyBody")}</Text>
        <Badge appearance="tint" color="success">
          {t("noCloudRequired")}
        </Badge>
        <Text size={200}>{t("licenseSummary")}</Text>
        <Button
          size="small"
          onClick={() => void dataApi.thirdPartyNotices().then(setLicenseText).catch(reportError)}
        >
          {t("viewOpenSourceLicenses")}
        </Button>
      </section>
      {licenseText && (
        <Modal title={t("openSourceLicenses")} onClose={() => setLicenseText(null)}>
          <pre className="license-notices">{licenseText}</pre>
        </Modal>
      )}
    </div>
  );
}

export function BriefCard({
  date,
  locale,
  reportError,
}: {
  date: string;
  locale: string;
  reportError: (error: unknown) => void;
}) {
  const { t } = useTranslation();
  const [brief, setBrief] = useState<BriefResult | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <section className="content-card brief-card">
      <div className="section-title-row compact">
        <div>
          <Title2 as="h2">{t("morningBrief")}</Title2>
          <Text className="muted">{t("briefOptionalAi")}</Text>
        </div>
        <Button
          appearance={busy ? "secondary" : "primary"}
          onClick={() => {
            if (busy) {
              void runtimeApi.cancelRun().catch(reportError);
              return;
            }
            setBusy(true);
            void runtimeApi
              .morningBrief(date, locale)
              .then(setBrief)
              .catch(reportError)
              .finally(() => setBusy(false));
          }}
        >
          {busy ? t("cancelBrief") : t("generateAiBrief")}
        </Button>
      </div>
      {brief ? (
        <>
          <Title3 as="h3">{brief.oneSentenceGoal}</Title3>
          <div className="brief-priorities">
            {brief.priorities.map((item) => (
              <div key={item.taskId}>
                <strong>{item.reason}</strong>
                <Text size={200}>{item.firstStep}</Text>
              </div>
            ))}
          </div>
        </>
      ) : (
        <Text className="muted">{busy ? t("generatingCancelable") : t("briefNotGenerated")}</Text>
      )}
    </section>
  );
}

function Modal({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => event.target === event.currentTarget && onClose()}
    >
      <section className="composer" role="dialog" aria-modal="true">
        <div className="composer-header">
          <Title2 as="h2">{title}</Title2>
          <Button appearance="subtle" onClick={onClose}>
            ×
          </Button>
        </div>
        {children}
      </section>
    </div>
  );
}
function FormActions({
  t,
  onClose,
  saving,
}: {
  t: Translate;
  onClose: () => void;
  saving: boolean;
}) {
  return (
    <div className="composer-actions">
      <Button type="button" onClick={onClose}>
        {t("cancel")}
      </Button>
      <Button appearance="primary" type="submit" disabled={saving}>
        {saving ? t("saving") : t("save")}
      </Button>
    </div>
  );
}
function Setting({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{label}</strong>
        <Text size={200} className="muted">
          {hint}
        </Text>
      </div>
      {children}
    </div>
  );
}
function splitTerms(value: string) {
  return value
    .split(/[,，\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}
function moveId(ids: string[], id: string, delta: number) {
  const from = ids.indexOf(id);
  const to = Math.max(0, Math.min(ids.length - 1, from + delta));
  if (from < 0 || from === to) return ids;
  const copy = [...ids];
  const [item] = copy.splice(from, 1);
  copy.splice(to, 0, item);
  return copy;
}
function formatTimestamp(value: string | null, locale: string | undefined, fallback: string) {
  return value
    ? new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(
        new Date(value),
      )
    : fallback;
}
function formatBytes(value: number) {
  return value < 1024 * 1024
    ? `${Math.round(value / 1024)} KB`
    : `${(value / 1024 / 1024).toFixed(1)} MB`;
}
function localDateString(value: Date) {
  const year = value.getFullYear();
  const month = String(value.getMonth() + 1).padStart(2, "0");
  const day = String(value.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}
function applyTheme(theme: Preferences["theme"]) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme === "system" ? "light dark" : theme;
}
