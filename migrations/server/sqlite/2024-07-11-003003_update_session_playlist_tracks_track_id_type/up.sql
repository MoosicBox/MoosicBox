ALTER TABLE session_playlist_tracks ADD COLUMN new_track_id VARCHAR(64) DEFAULT NULL;
UPDATE session_playlist_tracks SET new_track_id = CAST(track_id as VARCHAR(64));
ALTER TABLE session_playlist_tracks DROP COLUMN track_id;
ALTER TABLE session_playlist_tracks ADD COLUMN track_id VARCHAR(64) DEFAULT NULL;
UPDATE session_playlist_tracks SET track_id = new_track_id;
ALTER TABLE session_playlist_tracks DROP COLUMN new_track_id;
