PRAGMA foreign_keys = ON;

ALTER TABLE knowledge_nodes
    ADD COLUMN stage_outcome TEXT NOT NULL DEFAULT '';

ALTER TABLE knowledge_nodes
    ADD COLUMN estimated_minutes INTEGER
        CHECK (estimated_minutes IS NULL OR estimated_minutes BETWEEN 1 AND 10080);

INSERT INTO schema_migrations(version, applied_at_utc)
VALUES (5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
