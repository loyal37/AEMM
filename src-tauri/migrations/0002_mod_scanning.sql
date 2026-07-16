ALTER TABLE mod_files
ADD COLUMN modified_at INTEGER NOT NULL DEFAULT 0;

ALTER TABLE mod_local_metadata
ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';

CREATE INDEX mod_files_modified_at_index
    ON mod_files (mod_id, modified_at);

CREATE INDEX mods_fingerprint_index
    ON mods (content_fingerprint);

CREATE UNIQUE INDEX mods_repository_path_nocase_unique
    ON mods (repository_path COLLATE NOCASE);
