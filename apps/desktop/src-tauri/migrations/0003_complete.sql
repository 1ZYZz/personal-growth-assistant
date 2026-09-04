ALTER TABLE tasks ADD COLUMN progress_percent INTEGER CHECK (progress_percent IS NULL OR progress_percent BETWEEN 0 AND 100);
ALTER TABLE tasks ADD COLUMN progress_note TEXT;
ALTER TABLE tasks ADD COLUMN completion_criteria TEXT;
ALTER TABLE watch_sources ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS digests (
    id TEXT PRIMARY KEY,
    digest_type TEXT NOT NULL CHECK (digest_type IN ('morning', 'evening', 'weekly', 'industry')),
    period_start TEXT NOT NULL,
    period_end TEXT NOT NULL,
    content_json TEXT NOT NULL CHECK (json_valid(content_json)),
    generator TEXT NOT NULL DEFAULT 'local-rules',
    created_at_utc TEXT NOT NULL,
    UNIQUE (digest_type, period_start, period_end)
);

CREATE TABLE IF NOT EXISTS runtime_configs (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL DEFAULT 'codex' CHECK (provider IN ('codex', 'openai_api', 'local')),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    executable_path TEXT,
    daily_run_limit INTEGER NOT NULL DEFAULT 2 CHECK (daily_run_limit BETWEEN 0 AND 20),
    timeout_seconds INTEGER NOT NULL DEFAULT 180 CHECK (timeout_seconds BETWEEN 10 AND 1800),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (provider)
);

CREATE TABLE IF NOT EXISTS ai_runs (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    job_kind TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'cancelled', 'timed_out')),
    input_fingerprint TEXT NOT NULL,
    output_json TEXT CHECK (output_json IS NULL OR json_valid(output_json)),
    error_message TEXT,
    started_at_utc TEXT NOT NULL,
    finished_at_utc TEXT
);

CREATE TABLE IF NOT EXISTS ai_usage (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES ai_runs(id),
    usage_kind TEXT NOT NULL,
    input_tokens INTEGER,
    cached_input_tokens INTEGER,
    output_tokens INTEGER,
    exact INTEGER NOT NULL DEFAULT 0 CHECK (exact IN (0, 1)),
    recorded_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS proposals (
    id TEXT PRIMARY KEY,
    proposal_type TEXT NOT NULL,
    diff_json TEXT NOT NULL CHECK (json_valid(diff_json)),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'expired', 'applied')),
    entity_version INTEGER,
    created_at_utc TEXT NOT NULL,
    expires_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS approvals (
    id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    token_hash TEXT NOT NULL UNIQUE,
    expires_at_utc TEXT NOT NULL,
    consumed_at_utc TEXT,
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    entity_type TEXT,
    entity_id TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    created_at_utc TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ai_runs_started ON ai_runs(started_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_watch_sources_sort ON watch_sources(field_id, sort_order, created_at_utc);
CREATE INDEX IF NOT EXISTS idx_tasks_deleted ON tasks(deleted_at_utc);

INSERT OR IGNORE INTO schema_migrations(version, applied_at_utc)
VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
