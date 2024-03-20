ALTER TABLE track_sizes ADD COLUMN new_bytes INTEGER DEFAULT NULL;
UPDATE track_sizes SET new_bytes = bytes;
ALTER TABLE track_sizes DROP COLUMN bytes;
ALTER TABLE track_sizes ADD COLUMN bytes INTEGER DEFAULT NULL;
UPDATE track_sizes SET bytes = new_bytes;
ALTER TABLE track_sizes DROP COLUMN new_bytes;

UPDATE track_sizes set bytes = NULL where bytes = 0;
