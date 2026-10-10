-- Logical Library membership is independent of physical source ownership.
ALTER TABLE media ADD COLUMN library_assignment TEXT;
CREATE INDEX media_library_assignment ON media(library_assignment, availability, needs_distribution);

-- One operation at a time; retain intent until both database and filesystem agree.
CREATE TABLE file_operations (
    media_id INTEGER PRIMARY KEY REFERENCES media(id) ON DELETE RESTRICT,
    source_root TEXT NOT NULL,
    source_relative TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    target_source INTEGER NOT NULL REFERENCES sources(id) ON DELETE RESTRICT,
    target_root TEXT NOT NULL,
    target_relative TEXT NOT NULL
) STRICT;
