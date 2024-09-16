DROP INDEX IF EXISTS ux_download_tasks;

ALTER TABLE download_tasks ADD COLUMN old_artist_id INTEGER DEFAULT NULL;
UPDATE download_tasks SET old_artist_id = CAST(artist_id as INTEGER);
ALTER TABLE download_tasks DROP COLUMN artist_id;
ALTER TABLE download_tasks ADD COLUMN artist_id INTEGER DEFAULT NULL;
UPDATE download_tasks SET artist_id = old_artist_id;
ALTER TABLE download_tasks DROP COLUMN old_artist_id;

ALTER TABLE download_tasks ADD COLUMN old_album_id INTEGER DEFAULT NULL;
UPDATE download_tasks SET old_album_id = CAST(album_id as INTEGER);
ALTER TABLE download_tasks DROP COLUMN album_id;
ALTER TABLE download_tasks ADD COLUMN album_id INTEGER DEFAULT NULL;
UPDATE download_tasks SET album_id = old_album_id;
ALTER TABLE download_tasks DROP COLUMN old_album_id;

ALTER TABLE download_tasks ADD COLUMN old_track_id INTEGER DEFAULT NULL;
UPDATE download_tasks SET old_track_id = CAST(track_id as INTEGER);
ALTER TABLE download_tasks DROP COLUMN track_id;
ALTER TABLE download_tasks ADD COLUMN track_id INTEGER DEFAULT NULL;
UPDATE download_tasks SET track_id = old_track_id;
ALTER TABLE download_tasks DROP COLUMN old_track_id;

CREATE UNIQUE INDEX ux_download_tasks ON download_tasks(
    `type`,
    ifnull(`track_id`, 0),
    ifnull(`album_id`, 0),
    ifnull(`source`, ''),
    ifnull(`quality`, ''),
    `file_path`,
    ifnull(`total_bytes`, 0)
);
