CREATE TABLE metadata_presets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    metadata TEXT NOT NULL CHECK (json_valid(metadata))
);
