CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY NOT NULL CHECK (version > 0),
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  checksum TEXT NOT NULL CHECK (length(checksum) = 64),
  applied_at TEXT NOT NULL
);

ALTER TABLE storage_meta DROP COLUMN schema_fingerprint;
