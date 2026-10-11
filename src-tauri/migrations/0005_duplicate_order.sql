ALTER TABLE media ADD COLUMN naming_order_key TEXT;
ALTER TABLE media ADD COLUMN naming_order INTEGER CHECK (naming_order IS NULL OR naming_order >= 0);
