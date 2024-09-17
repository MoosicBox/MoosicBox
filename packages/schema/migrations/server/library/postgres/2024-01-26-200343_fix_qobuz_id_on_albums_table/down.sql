ALTER TABLE albums ADD COLUMN old_qobuz_id BIGINT DEFAULT NULL;
UPDATE albums SET old_qobuz_id = CAST(qobuz_id as BIGINT);
ALTER TABLE albums DROP COLUMN qobuz_id;
ALTER TABLE albums ADD COLUMN qobuz_id BIGINT DEFAULT NULL;
UPDATE albums SET qobuz_id = old_qobuz_id;
ALTER TABLE albums DROP COLUMN old_qobuz_id;
