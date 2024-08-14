ALTER TABLE sessions ADD COLUMN playback_target VARCHAR(128) DEFAULT NULL;
ALTER TABLE sessions ADD COLUMN connection_id VARCHAR(128) DEFAULT NULL;
ALTER TABLE sessions ADD COLUMN output_id VARCHAR(128) DEFAULT NULL;

UPDATE sessions SET playback_target = 'AUDIO_ZONE' WHERE audio_zone_id is not null;
