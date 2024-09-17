ALTER TABLE albums ADD COLUMN new_qobuz_id VARCHAR(64) DEFAULT NULL;
UPDATE albums SET new_qobuz_id = CAST(qobuz_id as VARCHAR(64));
ALTER TABLE albums DROP COLUMN qobuz_id;
ALTER TABLE albums ADD COLUMN qobuz_id VARCHAR(64) DEFAULT NULL;
UPDATE albums SET qobuz_id = new_qobuz_id;
ALTER TABLE albums DROP COLUMN new_qobuz_id;
