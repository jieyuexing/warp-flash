DROP INDEX tabs_archive_id_idx;

ALTER TABLE tabs DROP COLUMN archived_at;
ALTER TABLE tabs DROP COLUMN archived;
ALTER TABLE tabs DROP COLUMN archive_id;
