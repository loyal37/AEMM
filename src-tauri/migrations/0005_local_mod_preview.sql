ALTER TABLE mod_local_metadata
ADD COLUMN preview_file_name TEXT
    CHECK (
        preview_file_name IS NULL
        OR (
            length(preview_file_name) BETWEEN 1 AND 128
            AND instr(preview_file_name, '/') = 0
            AND instr(preview_file_name, '\') = 0
        )
    );
