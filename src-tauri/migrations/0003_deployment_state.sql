CREATE TABLE app_state (
    singleton INTEGER PRIMARY KEY NOT NULL DEFAULT 1 CHECK (singleton = 1),
    active_profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    updated_at INTEGER NOT NULL
);

INSERT INTO app_state (singleton, active_profile_id, updated_at)
VALUES (
    1,
    '00000000-0000-0000-0000-000000000001',
    CAST(strftime('%s', 'now') AS INTEGER)
);

CREATE INDEX profile_mods_enabled_index
    ON profile_mods (profile_id, enabled, load_order);

CREATE INDEX deployment_records_profile_index
    ON deployment_records (profile_id, created_at);
