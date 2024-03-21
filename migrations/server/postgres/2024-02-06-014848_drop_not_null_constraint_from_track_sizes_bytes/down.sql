ALTER TABLE track_sizes ADD COLUMN old_bytes BIGINT NOT NULL DEFAULT 0;
UPDATE track_sizes SET old_bytes = COALESCE(bytes, 0);
ALTER TABLE track_sizes DROP COLUMN bytes;
ALTER TABLE track_sizes ADD COLUMN bytes BIGINT NOT NULL DEFAULT 0;
UPDATE track_sizes SET bytes = old_bytes;
ALTER TABLE track_sizes DROP COLUMN old_bytes;
