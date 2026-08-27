ALTER TABLE tabs ADD COLUMN archive_id TEXT;
ALTER TABLE tabs ADD COLUMN archived BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE tabs ADD COLUMN archived_at BIGINT;

CREATE UNIQUE INDEX tabs_archive_id_idx
ON tabs (archive_id)
WHERE archive_id IS NOT NULL;
