CREATE TABLE IF NOT EXISTS trash (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    snapshot_json TEXT NOT NULL CHECK (json_valid(snapshot_json)),
    deleted_at_utc TEXT NOT NULL,
    purge_after_utc TEXT NOT NULL,
    restored_at_utc TEXT,
    UNIQUE (entity_type, entity_id, deleted_at_utc)
);

CREATE TABLE IF NOT EXISTS reminder_rules (
    id TEXT PRIMARY KEY,
    task_id TEXT REFERENCES tasks(id),
    series_id TEXT REFERENCES recurrence_series(id),
    minutes_before INTEGER NOT NULL CHECK (minutes_before >= 0 AND minutes_before <= 10080),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK ((task_id IS NOT NULL AND series_id IS NULL) OR (task_id IS NULL AND series_id IS NOT NULL)),
    UNIQUE (task_id),
    UNIQUE (series_id)
);

ALTER TABLE reminders ADD COLUMN reminder_rule_id TEXT REFERENCES reminder_rules(id);

CREATE TABLE IF NOT EXISTS notification_nonces (
    id TEXT PRIMARY KEY,
    notification_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('complete', 'snooze', 'open')),
    nonce_hash TEXT NOT NULL UNIQUE,
    expires_at_utc TEXT NOT NULL,
    consumed_at_utc TEXT,
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS quiet_hours (
    id TEXT PRIMARY KEY,
    start_local_time TEXT NOT NULL,
    end_local_time TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    updated_at_utc TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS watch_fields (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description TEXT NOT NULL DEFAULT '',
    include_terms_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(include_terms_json)),
    exclude_terms_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(exclude_terms_json)),
    regions_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(regions_json)),
    languages_json TEXT NOT NULL DEFAULT '["zh-CN","en"]' CHECK (json_valid(languages_json)),
    max_items INTEGER NOT NULL DEFAULT 5 CHECK (max_items BETWEEN 1 AND 20),
    reading_minutes INTEGER NOT NULL DEFAULT 10 CHECK (reading_minutes BETWEEN 1 AND 120),
    relevance_weight REAL NOT NULL DEFAULT 0.35,
    recency_weight REAL NOT NULL DEFAULT 0.20,
    authority_weight REAL NOT NULL DEFAULT 0.25 CHECK (authority_weight > 0),
    heat_weight REAL NOT NULL DEFAULT 0.10,
    breaking_alerts INTEGER NOT NULL DEFAULT 0 CHECK (breaking_alerts IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS watch_sources (
    id TEXT PRIMARY KEY,
    field_id TEXT NOT NULL REFERENCES watch_fields(id),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    url TEXT NOT NULL,
    source_type TEXT NOT NULL DEFAULT 'rss' CHECK (source_type IN ('rss', 'atom', 'api', 'page')),
    authority REAL NOT NULL DEFAULT 0.8 CHECK (authority BETWEEN 0 AND 1),
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    etag TEXT,
    last_modified TEXT,
    last_success_at_utc TEXT,
    last_error TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (field_id, url)
);

CREATE TABLE IF NOT EXISTS source_fetches (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES watch_sources(id),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'not_modified', 'failed')),
    http_status INTEGER,
    input_count INTEGER NOT NULL DEFAULT 0,
    output_count INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    error_message TEXT,
    started_at_utc TEXT NOT NULL,
    finished_at_utc TEXT
);

CREATE TABLE IF NOT EXISTS source_snapshots (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES watch_sources(id),
    fetch_id TEXT NOT NULL REFERENCES source_fetches(id),
    content_hash TEXT NOT NULL,
    normalized_text TEXT,
    captured_at_utc TEXT NOT NULL,
    UNIQUE (source_id, content_hash)
);

CREATE TABLE IF NOT EXISTS news_items (
    id TEXT PRIMARY KEY,
    field_id TEXT NOT NULL REFERENCES watch_fields(id),
    source_id TEXT NOT NULL REFERENCES watch_sources(id),
    external_id TEXT,
    canonical_url TEXT NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    summary TEXT NOT NULL DEFAULT '',
    published_at_utc TEXT,
    event_date TEXT,
    information_kind TEXT NOT NULL DEFAULT 'media_report' CHECK (information_kind IN ('official_statement', 'research_result', 'media_report', 'analysis_inference')),
    relevance_score REAL NOT NULL DEFAULT 0,
    authority_score REAL NOT NULL DEFAULT 0,
    recency_score REAL NOT NULL DEFAULT 0,
    novelty_score REAL NOT NULL DEFAULT 0,
    corroboration_score REAL NOT NULL DEFAULT 0,
    score REAL NOT NULL DEFAULT 0,
    uncertainty TEXT,
    content_hash TEXT NOT NULL,
    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
    is_saved INTEGER NOT NULL DEFAULT 0 CHECK (is_saved IN (0, 1)),
    feedback TEXT CHECK (feedback IS NULL OR feedback IN ('not_interested')),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (source_id, content_hash)
);

CREATE TABLE IF NOT EXISTS learning_goals (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    purpose TEXT NOT NULL DEFAULT '',
    current_level TEXT NOT NULL DEFAULT 'beginner',
    target_level TEXT NOT NULL DEFAULT 'competent',
    target_date TEXT,
    weekly_minutes INTEGER NOT NULL DEFAULT 180 CHECK (weekly_minutes BETWEEN 10 AND 10080),
    daily_minutes INTEGER NOT NULL DEFAULT 30 CHECK (daily_minutes BETWEEN 5 AND 1440),
    language TEXT NOT NULL DEFAULT 'zh-CN',
    resource_preferences_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(resource_preferences_json)),
    budget_mode TEXT NOT NULL DEFAULT 'free_first' CHECK (budget_mode IN ('free_first', 'paid_allowed')),
    auto_add_lessons INTEGER NOT NULL DEFAULT 0 CHECK (auto_add_lessons IN (0, 1)),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'completed', 'paused', 'cancelled')),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS knowledge_nodes (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL REFERENCES learning_goals(id),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    plain_explanation TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'not_started' CHECK (status IN ('not_started', 'learning', 'review_due', 'basic', 'mastered')),
    mastery_score INTEGER NOT NULL DEFAULT 0 CHECK (mastery_score BETWEEN 0 AND 100),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    deleted_at_utc TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS knowledge_edges (
    prerequisite_node_id TEXT NOT NULL REFERENCES knowledge_nodes(id),
    dependent_node_id TEXT NOT NULL REFERENCES knowledge_nodes(id),
    created_at_utc TEXT NOT NULL,
    PRIMARY KEY (prerequisite_node_id, dependent_node_id),
    CHECK (prerequisite_node_id != dependent_node_id)
);

CREATE TABLE IF NOT EXISTS learning_resources (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES knowledge_nodes(id),
    title TEXT NOT NULL,
    author TEXT,
    published_date TEXT,
    resource_type TEXT NOT NULL,
    language TEXT,
    duration_minutes INTEGER,
    difficulty TEXT,
    cost TEXT,
    version_fit TEXT,
    stale_risk TEXT,
    url TEXT NOT NULL,
    recommendation_reason TEXT NOT NULL DEFAULT '',
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS quiz_items (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES knowledge_nodes(id),
    question_type TEXT NOT NULL CHECK (question_type IN ('single', 'multiple', 'true_false', 'short_answer', 'calculation', 'code', 'practice')),
    prompt TEXT NOT NULL,
    options_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(options_json)),
    answer_json TEXT NOT NULL CHECK (json_valid(answer_json)),
    explanation TEXT NOT NULL DEFAULT '',
    difficulty INTEGER NOT NULL DEFAULT 1 CHECK (difficulty BETWEEN 1 AND 5),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS quiz_attempts (
    id TEXT PRIMARY KEY,
    quiz_item_id TEXT NOT NULL REFERENCES quiz_items(id),
    answer_json TEXT NOT NULL CHECK (json_valid(answer_json)),
    is_correct INTEGER CHECK (is_correct IS NULL OR is_correct IN (0, 1)),
    confidence TEXT CHECK (confidence IS NULL OR confidence IN ('sure', 'unsure', 'skipped')),
    hints_used INTEGER NOT NULL DEFAULT 0,
    attempted_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mistake_book (
    id TEXT PRIMARY KEY,
    quiz_item_id TEXT NOT NULL REFERENCES quiz_items(id),
    latest_attempt_id TEXT NOT NULL REFERENCES quiz_attempts(id),
    resolved_at_utc TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (quiz_item_id)
);

CREATE TABLE IF NOT EXISTS fsrs_cards (
    id TEXT PRIMARY KEY,
    quiz_item_id TEXT NOT NULL UNIQUE REFERENCES quiz_items(id),
    due_utc TEXT NOT NULL,
    stability REAL NOT NULL DEFAULT 0,
    difficulty REAL NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'new',
    reps INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    last_review_utc TEXT,
    algorithm_version TEXT NOT NULL DEFAULT 'baseline-v1',
    params_version TEXT NOT NULL DEFAULT 'default-v1',
    updated_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS review_logs (
    id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL REFERENCES fsrs_cards(id),
    rating INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 4),
    previous_due_utc TEXT NOT NULL,
    next_due_utc TEXT NOT NULL,
    reviewed_at_utc TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mastery_evidence (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES knowledge_nodes(id),
    evidence_type TEXT NOT NULL,
    score_delta INTEGER NOT NULL,
    source_id TEXT,
    created_at_utc TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_trash_purge ON trash(restored_at_utc, purge_after_utc);
CREATE UNIQUE INDEX IF NOT EXISTS idx_reminder_occurrence_rule
    ON reminders(reminder_rule_id, occurrence_series_id, occurrence_key)
    WHERE reminder_rule_id IS NOT NULL AND occurrence_series_id IS NOT NULL AND occurrence_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_news_field_score ON news_items(field_id, score DESC, published_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_source_field ON watch_sources(field_id, enabled);
CREATE INDEX IF NOT EXISTS idx_nodes_goal ON knowledge_nodes(goal_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_cards_due ON fsrs_cards(due_utc);

INSERT OR IGNORE INTO schema_migrations(version, applied_at_utc)
VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
