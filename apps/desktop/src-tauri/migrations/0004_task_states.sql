PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

CREATE TABLE tasks_v4 (
    id TEXT PRIMARY KEY,
    series_id TEXT REFERENCES recurrence_series(id),
    occurrence_key TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    notes TEXT,
    priority TEXT NOT NULL DEFAULT 'p2' CHECK (priority IN ('p0', 'p1', 'p2', 'p3')),
    estimated_minutes INTEGER CHECK (estimated_minutes IS NULL OR estimated_minutes > 0),
    project_id TEXT REFERENCES projects(id),
    blocked_reason TEXT,
    snoozed_until_utc TEXT,
    status TEXT NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'in_progress', 'snoozed', 'blocked', 'completed', 'canceled')),
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
    progress_percent INTEGER CHECK (progress_percent IS NULL OR progress_percent BETWEEN 0 AND 100),
    progress_note TEXT,
    completion_criteria TEXT,
    CHECK ((series_id IS NULL AND occurrence_key IS NULL) OR (series_id IS NOT NULL AND occurrence_key IS NOT NULL)),
    CHECK ((time_mode = 'all_day' AND scheduled_date IS NOT NULL) OR (time_mode != 'all_day' AND scheduled_local IS NOT NULL)),
    CHECK (status != 'blocked' OR (blocked_reason IS NOT NULL AND length(trim(blocked_reason)) > 0)),
    CHECK (status != 'snoozed' OR snoozed_until_utc IS NOT NULL),
    UNIQUE (series_id, occurrence_key)
);

INSERT INTO tasks_v4(
    id, series_id, occurrence_key, title, notes, priority, estimated_minutes,
    project_id, blocked_reason, status, time_mode, scheduled_local, scheduled_date,
    scheduled_utc, tzid, chosen_offset_seconds, dst_adjusted, version,
    created_at_utc, updated_at_utc, completed_at_utc, deleted_at_utc,
    progress_percent, progress_note, completion_criteria
)
SELECT
    id, series_id, occurrence_key, title, notes, priority, estimated_minutes,
    project_id, blocked_reason,
    CASE WHEN status = 'cancelled' THEN 'canceled' ELSE status END,
    time_mode, scheduled_local, scheduled_date, scheduled_utc, tzid,
    chosen_offset_seconds, dst_adjusted, version, created_at_utc, updated_at_utc,
    completed_at_utc, deleted_at_utc, progress_percent, progress_note,
    completion_criteria
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_v4 RENAME TO tasks;

CREATE INDEX idx_tasks_schedule ON tasks(status, scheduled_utc) WHERE deleted_at_utc IS NULL;
CREATE INDEX idx_tasks_series ON tasks(series_id, occurrence_key) WHERE deleted_at_utc IS NULL;
CREATE INDEX idx_tasks_deleted ON tasks(deleted_at_utc);
CREATE INDEX idx_tasks_snoozed ON tasks(status, snoozed_until_utc) WHERE deleted_at_utc IS NULL;

INSERT INTO schema_migrations(version, applied_at_utc)
VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

COMMIT;
PRAGMA foreign_keys = ON;
