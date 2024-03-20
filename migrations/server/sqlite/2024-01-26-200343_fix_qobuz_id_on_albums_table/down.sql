ALTER TABLE albums ADD COLUMN old_qobuz_id INTEGER DEFAULT NULL;
UPDATE albums SET old_qobuz_id = CAST(qobuz_id as INTEGER);
ALTER TABLE albums DROP COLUMN qobuz_id;
ALTER TABLE albums ADD COLUMN qobuz_id INTEGER DEFAULT NULL;
UPDATE albums SET qobuz_id = old_qobuz_id;
ALTER TABLE albums DROP COLUMN old_qobuz_id;
