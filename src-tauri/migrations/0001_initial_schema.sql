PRAGMA foreign_keys = ON;

CREATE TABLE mods (
    id TEXT PRIMARY KEY NOT NULL,
    logical_id TEXT NOT NULL,
    repository_path TEXT NOT NULL UNIQUE,
    content_fingerprint TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
    installed_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    lifecycle_state TEXT NOT NULL DEFAULT 'installed'
        CHECK (lifecycle_state IN ('installing', 'installed', 'broken', 'removing'))
);

CREATE UNIQUE INDEX mods_logical_id_unique
    ON mods (logical_id COLLATE NOCASE);
CREATE INDEX mods_installed_at_index ON mods (installed_at DESC);
CREATE INDEX mods_updated_at_index ON mods (updated_at DESC);

CREATE TABLE mod_author_metadata (
    mod_id TEXT PRIMARY KEY NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    author TEXT,
    version TEXT,
    description TEXT,
    category TEXT,
    game_version TEXT,
    website TEXT,
    preview_path TEXT,
    original_json TEXT,
    source_kind TEXT NOT NULL DEFAULT 'inferred'
        CHECK (source_kind IN ('mod_json', 'inferred'))
);

CREATE TABLE mod_local_metadata (
    mod_id TEXT PRIMARY KEY NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    display_name_override TEXT,
    category_override TEXT,
    description_override TEXT,
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    notes TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE mod_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    deployment_target TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
    content_hash TEXT,
    file_role TEXT NOT NULL DEFAULT 'content',
    UNIQUE (mod_id, source_path)
);

CREATE INDEX mod_files_target_index ON mod_files (deployment_target);
CREATE INDEX mod_files_hash_index ON mod_files (content_hash);

CREATE TABLE profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE profile_mods (
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    load_order INTEGER NOT NULL CHECK (load_order >= 0),
    PRIMARY KEY (profile_id, mod_id),
    UNIQUE (profile_id, load_order)
);

CREATE TABLE deployment_records (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE,
    strategy_id TEXT NOT NULL,
    destination_root TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (profile_id, mod_id)
);

INSERT INTO profiles (id, name, created_at, updated_at)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    '默认配置',
    CAST(strftime('%s', 'now') AS INTEGER),
    CAST(strftime('%s', 'now') AS INTEGER)
);
