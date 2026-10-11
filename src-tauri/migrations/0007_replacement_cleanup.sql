-- Preserve the retention policy of existing operations/history. New replacements
-- explicitly select deletion after durable adoption and recoverable cleanup.
ALTER TABLE media_replacements ADD COLUMN delete_old INTEGER NOT NULL DEFAULT 0
    CHECK (delete_old IN (0, 1));
