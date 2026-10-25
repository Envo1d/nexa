CREATE TABLE duplicate_fingerprints (
    media_id INTEGER PRIMARY KEY REFERENCES media(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('image', 'video')),
    algorithm_version INTEGER NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256)=64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    visual TEXT CHECK (visual IS NULL OR (length(visual)=16 AND visual NOT GLOB '*[^0-9a-f]*')),
    visual_retry INTEGER NOT NULL CHECK (visual_retry IN (0,1)),
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE duplicate_searches (
    scope TEXT PRIMARY KEY,
    generation INTEGER NOT NULL CHECK (generation > 0),
    algorithm_version INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('partial','complete','stopped','limited','failed')),
    report_json TEXT NOT NULL CHECK (json_valid(report_json)),
    pairs_json TEXT NOT NULL CHECK (json_valid(pairs_json))
) STRICT;
