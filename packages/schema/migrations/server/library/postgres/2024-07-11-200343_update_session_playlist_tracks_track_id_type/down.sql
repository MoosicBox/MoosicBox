ALTER TABLE session_playlist_tracks ADD COLUMN old_track_id BIGINT DEFAULT NULL;
UPDATE session_playlist_tracks SET old_track_id = CAST(track_id as BIGINT);
ALTER TABLE session_playlist_tracks DROP COLUMN track_id;
ALTER TABLE session_playlist_tracks ADD COLUMN track_id BIGINT DEFAULT NULL;
UPDATE session_playlist_tracks SET track_id = old_track_id;
ALTER TABLE session_playlist_tracks DROP COLUMN old_track_id;
