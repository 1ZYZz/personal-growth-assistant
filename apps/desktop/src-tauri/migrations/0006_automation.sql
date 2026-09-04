PRAGMA foreign_keys = ON;

ALTER TABLE watch_sources ADD COLUMN selector TEXT;
ALTER TABLE recurrence_series ADD COLUMN completion_criteria TEXT;
ALTER TABLE runtime_configs ADD COLUMN monthly_run_limit INTEGER NOT NULL DEFAULT 40
    CHECK (monthly_run_limit BETWEEN 0 AND 500);
ALTER TABLE runtime_configs ADD COLUMN budget_mode TEXT NOT NULL DEFAULT 'saving'
    CHECK (budget_mode IN ('saving', 'standard', 'deep'));

CREATE TABLE IF NOT EXISTS ingestion_runs (
    id TEXT PRIMARY KEY,
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('manual', 'scheduled')),
    field_id TEXT REFERENCES watch_fields(id),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'partial', 'failed')),
    source_count INTEGER NOT NULL DEFAULT 0,
    succeeded_count INTEGER NOT NULL DEFAULT 0,
    failed_count INTEGER NOT NULL DEFAULT 0,
    new_item_count INTEGER NOT NULL DEFAULT 0,
    started_at_utc TEXT NOT NULL,
    finished_at_utc TEXT
);

CREATE TABLE IF NOT EXISTS ingestion_stages (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES ingestion_runs(id),
    stage TEXT NOT NULL CHECK (stage IN ('fetch', 'normalize', 'deduplicate', 'rank', 'verify', 'store')),
    input_count INTEGER NOT NULL DEFAULT 0,
    output_count INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS lesson_plans (
    id TEXT PRIMARY KEY,
    plan_date TEXT NOT NULL,
    goal_id TEXT NOT NULL REFERENCES learning_goals(id),
    budget_minutes INTEGER NOT NULL CHECK (budget_minutes BETWEEN 0 AND 1440),
    planned_minutes INTEGER NOT NULL CHECK (planned_minutes BETWEEN 0 AND 1440),
    task_load TEXT NOT NULL CHECK (task_load IN ('light', 'normal', 'heavy')),
    status TEXT NOT NULL DEFAULT 'suggested' CHECK (status IN ('suggested', 'approved', 'completed', 'dismissed')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (plan_date, goal_id)
);

CREATE TABLE IF NOT EXISTS lesson_plan_items (
    id TEXT PRIMARY KEY,
    plan_id TEXT NOT NULL REFERENCES lesson_plans(id),
    node_id TEXT REFERENCES knowledge_nodes(id),
    item_kind TEXT NOT NULL CHECK (item_kind IN ('review', 'learn', 'practice', 'quiz')),
    title TEXT NOT NULL,
    minutes INTEGER NOT NULL CHECK (minutes BETWEEN 1 AND 240),
    sort_order INTEGER NOT NULL,
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_logs (
    id TEXT PRIMARY KEY,
    category TEXT NOT NULL CHECK (category IN ('app', 'scheduler', 'database', 'agent', 'ingestion', 'notification')),
    level TEXT NOT NULL CHECK (level IN ('info', 'warning', 'error')),
    event_name TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    message TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details_json)),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS proposal_applications (
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    task_id TEXT NOT NULL REFERENCES tasks(id),
    created_at_utc TEXT NOT NULL,
    PRIMARY KEY (proposal_id, task_id)
);

CREATE INDEX IF NOT EXISTS idx_ingestion_runs_started ON ingestion_runs(started_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_lesson_plans_date ON lesson_plans(plan_date DESC, goal_id);
CREATE INDEX IF NOT EXISTS idx_lesson_items_plan ON lesson_plan_items(plan_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_app_logs_recent ON app_logs(level, created_at_utc DESC);

INSERT INTO schema_migrations(version, applied_at_utc)
VALUES (6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
