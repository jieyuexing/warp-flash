# Persistent Agent Session Sidebar

## Goal

The right vertical-tabs panel is the durable control surface for parallel agent CLI sessions. A
locally restorable tab remains available after restart until the user explicitly deletes it. Pinning,
archiving, cross-window moves, and presentation changes must not create a new session identity.

Transient read-only terminal views remain outside the existing workspace snapshot scope because they
do not contain a locally restorable session layout.

## State model

Each tab owns one UUID from creation until deletion:

```text
active tab --archive--> archived tab --restore--> active tab
     |                       |
     +-------delete----------+
```

- Archive is lifecycle metadata on the same tab identity, not a second identity.
- Restore reuses the archived UUID and reinserts the tab into the pinned or grouped region.
- Cross-window transfer carries the UUID and pinned state to the destination.
- Delete is the only operation that intentionally ends the persisted tab lifecycle.
- Tab groups also retain their UUID so group membership can be reconstructed without remapping
  identity after every restart.

## Persistence

SQLite stores:

- `tabs.persistent_id`, `tabs.archived`, and `tabs.archived_at` for tab identity and lifecycle;
- `tab_groups.persistent_id` for stable group identity;
- `windows.vertical_tabs_panel_width` and `windows.archived_tabs_expanded` for sidebar presentation.

The migration renames the previous archived-only identifier column, preserving all existing archive
UUIDs. Legacy active tabs and groups without an identifier receive one when loaded and persist it on
the next app-state save.

The sidebar width is clamped to the current minimum and rejects non-finite persisted values. Width is
saved when dragging ends, and archive expansion is saved when toggled. A restored width at or below
the mini threshold renders icon-only active and archived session affordances.

## Verification contract

- Saving, loading, and saving again preserves active and archived tab UUIDs.
- A pre-migration archived UUID survives migration.
- Close, archive, and restore preserve one UUID.
- Pinning and stable tab-group UUIDs round-trip through SQLite.
- New-window and existing-window transfers retain identity and pinned ordering.
- Mini width and archive expansion survive app restart.
- The independent manual fixture is `/Users/jieyuexing/dsh-universe/.local/warp-flash-tests`.
