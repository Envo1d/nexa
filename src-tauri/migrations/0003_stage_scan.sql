-- Full precision revision detection and extractor versioning; retain existing IDs.
ALTER TABLE media ADD COLUMN file_revision TEXT;
ALTER TABLE media ADD COLUMN metadata_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE media ADD COLUMN stage_search_key TEXT;
CREATE INDEX media_stage_browse ON media(source_id, availability, id);
