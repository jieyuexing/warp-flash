DROP INDEX tabs_archive_id_idx;
ALTER TABLE tabs RENAME COLUMN archive_id TO persistent_id;
CREATE UNIQUE INDEX tabs_persistent_id_idx
ON tabs (persistent_id)
WHERE persistent_id IS NOT NULL;

ALTER TABLE tab_groups ADD COLUMN persistent_id TEXT;
CREATE UNIQUE INDEX tab_groups_persistent_id_idx
ON tab_groups (persistent_id)
WHERE persistent_id IS NOT NULL;

ALTER TABLE windows ADD COLUMN vertical_tabs_panel_width FLOAT;
ALTER TABLE windows ADD COLUMN archived_tabs_expanded BOOLEAN NOT NULL DEFAULT FALSE;
