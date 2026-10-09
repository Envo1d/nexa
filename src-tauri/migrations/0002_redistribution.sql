-- Keep physical source/path identity while exposing files for redistribution.
ALTER TABLE media ADD COLUMN needs_distribution INTEGER NOT NULL DEFAULT 0
    CHECK (needs_distribution IN (0, 1));
CREATE INDEX media_distribution ON media(needs_distribution, id) WHERE needs_distribution = 1;
