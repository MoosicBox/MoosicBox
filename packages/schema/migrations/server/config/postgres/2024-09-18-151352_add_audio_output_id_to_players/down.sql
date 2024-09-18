DELETE FROM players;

ALTER TABLE players DROP COLUMN audio_output_id;
ALTER TABLE players ADD COLUMN `type` TEXT NOT NULL;
