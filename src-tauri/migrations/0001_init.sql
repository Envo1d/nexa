-- Forward-only initial schema. Never edit this file after releasing it.
CREATE TABLE sources (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('library', 'stage')),
    root_path TEXT NOT NULL CHECK (length(trim(root_path)) > 0),
    path_key TEXT NOT NULL UNIQUE CHECK (length(path_key) > 0),
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    last_successful_scan_at INTEGER,
    created_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX sources_active_kind ON sources(kind) WHERE is_active = 1;

CREATE TABLE copyrights (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0 AND name = trim(name)),
    name_key TEXT NOT NULL UNIQUE CHECK (length(trim(name_key)) > 0 AND name_key = trim(name_key))
) STRICT;

CREATE TABLE artists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0 AND name = trim(name)),
    name_key TEXT NOT NULL UNIQUE CHECK (length(trim(name_key)) > 0 AND name_key = trim(name_key))
) STRICT;

CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0 AND name = trim(name)),
    name_key TEXT NOT NULL UNIQUE CHECK (length(trim(name_key)) > 0 AND name_key = trim(name_key))
) STRICT;

CREATE TABLE characters (
    id INTEGER PRIMARY KEY,
    copyright_id INTEGER NOT NULL REFERENCES copyrights(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0 AND name = trim(name)),
    name_key TEXT NOT NULL CHECK (length(trim(name_key)) > 0 AND name_key = trim(name_key)),
    UNIQUE (copyright_id, name_key),
    UNIQUE (id, copyright_id)
) STRICT;

CREATE INDEX characters_name ON characters(name_key, copyright_id);

CREATE TABLE media (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE RESTRICT,
    artist_id INTEGER REFERENCES artists(id) ON DELETE RESTRICT,
    relative_path TEXT NOT NULL CHECK (length(relative_path) > 0),
    original_filename TEXT NOT NULL CHECK (
        length(trim(original_filename)) > 0
        AND instr(original_filename, '/') = 0 AND instr(original_filename, char(92)) = 0
        AND original_filename NOT IN ('.', '..')
    ),
    path_key TEXT NOT NULL CHECK (length(path_key) > 0),
    kind TEXT NOT NULL CHECK (kind IN ('image', 'video')),
    extension TEXT NOT NULL CHECK (length(trim(extension)) > 0 AND instr(extension, '.') = 0),
    availability TEXT NOT NULL DEFAULT 'present' CHECK (availability IN ('present', 'missing')),
    size_bytes INTEGER CHECK (size_bytes >= 0),
    width INTEGER CHECK (width > 0),
    height INTEGER CHECK (height > 0),
    file_modified_at INTEGER,
    file_created_at INTEGER,
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    metadata_state TEXT NOT NULL DEFAULT 'pending' CHECK (metadata_state IN ('pending', 'ready', 'failed')),
    last_seen_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (source_id, path_key)
) STRICT;

CREATE INDEX media_source_modified ON media(source_id, file_modified_at, id);
CREATE INDEX media_artist ON media(artist_id, id);
CREATE INDEX media_favorites ON media(source_id, file_modified_at, id) WHERE favorite = 1;

CREATE TABLE video_metadata (
    media_id INTEGER PRIMARY KEY REFERENCES media(id) ON DELETE CASCADE,
    title TEXT CHECK (length(trim(title)) > 0 AND title = trim(title)),
    duration_ms INTEGER CHECK (duration_ms >= 0),
    fps_num INTEGER,
    fps_den INTEGER,
    codec TEXT CHECK (length(trim(codec)) > 0),
    bitrate_bps INTEGER CHECK (bitrate_bps > 0),
    CHECK (
        (fps_num IS NULL AND fps_den IS NULL)
        OR (fps_num IS NOT NULL AND fps_den IS NOT NULL AND fps_num > 0 AND fps_den > 0)
    )
) STRICT;

CREATE TRIGGER video_metadata_insert_kind
BEFORE INSERT ON video_metadata
WHEN NOT EXISTS (SELECT 1 FROM media WHERE id = NEW.media_id AND kind = 'video')
BEGIN
    SELECT RAISE(ABORT, 'video_metadata requires video media');
END;

CREATE TRIGGER video_metadata_update_kind
BEFORE UPDATE OF media_id ON video_metadata
WHEN NOT EXISTS (SELECT 1 FROM media WHERE id = NEW.media_id AND kind = 'video')
BEGIN
    SELECT RAISE(ABORT, 'video_metadata requires video media');
END;

CREATE TRIGGER media_kind_preserves_video_metadata
BEFORE UPDATE OF kind ON media
WHEN NEW.kind <> 'video' AND EXISTS (SELECT 1 FROM video_metadata WHERE media_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'remove video_metadata before changing media kind');
END;

CREATE TABLE media_copyrights (
    media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    copyright_id INTEGER NOT NULL REFERENCES copyrights(id) ON DELETE RESTRICT,
    PRIMARY KEY (media_id, copyright_id)
) STRICT;

CREATE TABLE media_tags (
    media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE RESTRICT,
    PRIMARY KEY (media_id, tag_id)
) STRICT;

CREATE TABLE media_characters (
    media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    character_id INTEGER NOT NULL,
    copyright_id INTEGER NOT NULL,
    PRIMARY KEY (media_id, character_id),
    FOREIGN KEY (character_id, copyright_id) REFERENCES characters(id, copyright_id) ON DELETE RESTRICT,
    FOREIGN KEY (media_id, copyright_id) REFERENCES media_copyrights(media_id, copyright_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX media_copyrights_reverse ON media_copyrights(copyright_id, media_id);
CREATE INDEX media_tags_reverse ON media_tags(tag_id, media_id);
CREATE INDEX media_characters_reverse ON media_characters(character_id, media_id);
CREATE INDEX media_characters_copyright ON media_characters(media_id, copyright_id);
