PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS recurrence_series (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    notes TEXT,
    priority TEXT NOT NULL DEFAULT 'p2' CHECK (priority IN ('p0', 'p1', 'p2', 'p3')),
    estimated_minutes INTEGER CHECK (estimated_minutes IS NULL OR estimated_minutes > 0),
    time_mode TEXT NOT NULL CHECK (time_mode IN ('all_day', 'floating', 'zoned')),
    dtstart_local TEXT NOT NULL,
    tzid TEXT,
    rrule_text TEXT NOT NULL,
    rdates_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(rdates_json)),
    exdates_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(exdates_json)),
    dst_gap_policy TEXT NOT NULL DEFAULT 'shift_forward' CHECK (dst_gap_policy = 'shift_forward'),
    dst_fold_policy TEXT NOT NULL DEFAULT 'earlier' CHECK (dst_fold_policy = 'earlier'),
    series_version INTEGER NOT NULL DEFAULT 1 CHECK (series_version > 0),
    ends_before_occurrence_key TEXT,
    next_occurrence_utc TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    CHECK (
        (time_mode = 'zoned' AND tzid IS NOT NULL AND length(tzid) > 0)
        OR (time_mode != 'zoned' AND tzid IS NULL)
    )
);

CREATE TABLE IF NOT EXISTS occurrence_overrides (
    id TEXT PRIMARY KEY,
    series_id TEXT NOT NULL REFERENCES recurrence_series(id),
    occurrence_key TEXT NOT NULL,
    patch_schema_version INTEGER NOT NULL DEFAULT 1 CHECK (patch_schema_version = 1),
    patch_json TEXT NOT NULL CHECK (json_valid(patch_json)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    UNIQUE (series_id, occurrence_key)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    series_id TEXT REFERENCES recurrence_series(id),
    occurrence_key TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    notes TEXT,
    priority TEXT NOT NULL DEFAULT 'p2' CHECK (priority IN ('p0', 'p1', 'p2', 'p3')),
    estimated_minutes INTEGER CHECK (estimated_minutes IS NULL OR estimated_minutes > 0),
    project_id TEXT REFERENCES projects(id),
    blocked_reason TEXT,
    status TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'completed', 'cancelled')),
    time_mode TEXT NOT NULL CHECK (time_mode IN ('all_day', 'floating', 'zoned')),
    scheduled_local TEXT,
    scheduled_date TEXT,
    scheduled_utc TEXT,
    tzid TEXT,
    chosen_offset_seconds INTEGER,
    dst_adjusted INTEGER NOT NULL DEFAULT 0 CHECK (dst_adjusted IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    completed_at_utc TEXT,
    deleted_at_utc TEXT,
    CHECK ((series_id IS NULL AND occurrence_key IS NULL) OR (series_id IS NOT NULL AND occurrence_key IS NOT NULL)),
    CHECK ((time_mode = 'all_day' AND scheduled_date IS NOT NULL) OR (time_mode != 'all_day' AND scheduled_local IS NOT NULL)),
    UNIQUE (series_id, occurrence_key)
);

CREATE TABLE IF NOT EXISTS user_profile (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL DEFAULT '',
    locale TEXT NOT NULL DEFAULT 'zh-CN' CHECK (locale IN ('zh-CN', 'en-US')),
    app_timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    updated_at_utc TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS goals (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'paused', 'cancelled')),
    target_date TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    goal_id TEXT REFERENCES goals(id),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'paused', 'cancelled')),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    depends_on_task_id TEXT NOT NULL REFERENCES tasks(id),
    created_at_utc TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on_task_id),
    CHECK (task_id != depends_on_task_id)
);

CREATE TABLE IF NOT EXISTS app_events (
    id TEXT PRIMARY KEY,
    category TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_events (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    event_type TEXT NOT NULL,
    occurrence_series_id TEXT,
    occurrence_key TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    created_at_utc TEXT NOT NULL,
    CHECK ((occurrence_series_id IS NULL AND occurrence_key IS NULL) OR (occurrence_series_id IS NOT NULL AND occurrence_key IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS recurrence_events (
    id TEXT PRIMARY KEY,
    series_id TEXT NOT NULL REFERENCES recurrence_series(id),
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reminders (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES tasks(id),
    occurrence_series_id TEXT,
    occurrence_key TEXT,
    remind_at_utc TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'leased', 'delivered', 'cancelled')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    CHECK (task_id IS NOT NULL OR (occurrence_series_id IS NOT NULL AND occurrence_key IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS notification_events (
    id TEXT PRIMARY KEY,
    reminder_id TEXT NOT NULL REFERENCES reminders(id),
    idempotency_key TEXT NOT NULL UNIQUE,
    action TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    owner TEXT NOT NULL DEFAULT 'local' CHECK (owner IN ('local', 'external')),
    business_key TEXT NOT NULL,
    scheduled_for_utc TEXT NOT NULL,
    attempt_class TEXT NOT NULL DEFAULT 'primary' CHECK (attempt_class IN ('primary', 'retry')),
    run_at_utc TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'leased', 'succeeded', 'failed', 'cancelled')),
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (business_key, scheduled_for_utc, attempt_class)
);

CREATE TABLE IF NOT EXISTS job_leases (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id),
    lease_owner TEXT NOT NULL,
    lease_token TEXT NOT NULL UNIQUE,
    run_id TEXT NOT NULL UNIQUE,
    leased_until_utc TEXT NOT NULL,
    acquired_at_utc TEXT NOT NULL,
    heartbeat_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS job_runs (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id),
    scheduled_for_utc TEXT NOT NULL,
    attempt_class TEXT NOT NULL CHECK (attempt_class IN ('primary', 'retry')),
    lease_token TEXT NOT NULL UNIQUE,
    outcome TEXT NOT NULL CHECK (outcome IN ('running', 'succeeded', 'failed', 'interrupted', 'cancelled', 'skipped')),
    error_message TEXT,
    started_at_utc TEXT NOT NULL,
    finished_at_utc TEXT,
    UNIQUE (job_id, scheduled_for_utc, attempt_class)
);

CREATE INDEX IF NOT EXISTS idx_tasks_schedule ON tasks(status, scheduled_utc) WHERE deleted_at_utc IS NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_series ON tasks(series_id, occurrence_key) WHERE deleted_at_utc IS NULL;
CREATE INDEX IF NOT EXISTS idx_overrides_series ON occurrence_overrides(series_id, occurrence_key) WHERE deleted_at_utc IS NULL;
CREATE INDEX IF NOT EXISTS idx_reminders_due ON reminders(status, remind_at_utc);
CREATE INDEX IF NOT EXISTS idx_jobs_due ON jobs(state, run_at_utc);

INSERT OR IGNORE INTO schema_migrations(version, applied_at_utc)
VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
