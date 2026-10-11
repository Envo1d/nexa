-- Replacement intent and retained provenance survive retirement of the Stage row.
ALTER TABLE video_metadata ADD COLUMN replacement_title_key TEXT;
CREATE TRIGGER video_replacement_title_key_changed
AFTER UPDATE OF title ON video_metadata
WHEN NEW.title IS NOT OLD.title
BEGIN
    UPDATE video_metadata SET replacement_title_key=NULL WHERE media_id=NEW.media_id;
END;

CREATE TABLE media_replacements (
    id INTEGER PRIMARY KEY,
    main_id INTEGER NOT NULL,
    stage_id INTEGER NOT NULL,
    old_source INTEGER NOT NULL,
    old_root TEXT NOT NULL,
    old_relative TEXT NOT NULL,
    old_revision TEXT NOT NULL,
    new_source INTEGER NOT NULL,
    new_root TEXT NOT NULL,
    new_relative TEXT NOT NULL,
    new_revision TEXT NOT NULL,
    incoming_filename TEXT NOT NULL,
    library_root TEXT NOT NULL,
    backup_root TEXT,
    committed INTEGER NOT NULL DEFAULT 0 CHECK (committed IN (0, 1)),
    finished INTEGER NOT NULL DEFAULT 0 CHECK (finished IN (0, 1)),
    created_at INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec')*1000 AS INTEGER))
) STRICT;
CREATE INDEX media_replacements_pending ON media_replacements(finished, id);
CREATE INDEX media_replacements_retained ON media_replacements(old_source, old_relative, old_revision)
    WHERE committed=1 AND backup_root IS NULL;
