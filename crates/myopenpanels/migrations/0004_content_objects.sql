ALTER TABLE storage_meta
ADD COLUMN content_format_version INTEGER NOT NULL DEFAULT 1
CHECK (content_format_version > 0);

UPDATE storage_meta
SET content_format_version = 2
WHERE id = 1;
