CREATE TABLE pending_mod_removals (
    mod_id TEXT PRIMARY KEY NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    original_repository_path TEXT NOT NULL,
    tombstone_repository_path TEXT NOT NULL COLLATE NOCASE UNIQUE,
    previous_lifecycle_state TEXT NOT NULL
        CHECK (previous_lifecycle_state IN ('installed', 'broken')),
    created_at INTEGER NOT NULL
);

CREATE INDEX pending_mod_removals_created_index
    ON pending_mod_removals (created_at, mod_id);
