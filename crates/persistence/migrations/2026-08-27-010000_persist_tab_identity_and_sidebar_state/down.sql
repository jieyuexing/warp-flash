ALTER TABLE windows DROP COLUMN archived_tabs_expanded;
ALTER TABLE windows DROP COLUMN vertical_tabs_panel_width;

DROP INDEX tab_groups_persistent_id_idx;
ALTER TABLE tab_groups DROP COLUMN persistent_id;

DROP INDEX tabs_persistent_id_idx;
ALTER TABLE tabs RENAME COLUMN persistent_id TO archive_id;
CREATE UNIQUE INDEX tabs_archive_id_idx
ON tabs (archive_id)
WHERE archive_id IS NOT NULL;
