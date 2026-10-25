-- Content-based visual review decisions survive renames and file record changes.
CREATE TABLE duplicate_rejections (
    left_hash TEXT NOT NULL CHECK (length(left_hash) = 64 AND left_hash NOT GLOB '*[^0-9a-f]*'),
    right_hash TEXT NOT NULL CHECK (length(right_hash) = 64 AND right_hash NOT GLOB '*[^0-9a-f]*'),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (left_hash, right_hash),
    CHECK (left_hash < right_hash)
) STRICT;
