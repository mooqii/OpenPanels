ALTER TABLE storage_meta
ADD COLUMN directory_layout_version INTEGER NOT NULL DEFAULT 1
CHECK (directory_layout_version > 0);

UPDATE storage_meta
SET directory_layout_version = 2
WHERE id = 1;
