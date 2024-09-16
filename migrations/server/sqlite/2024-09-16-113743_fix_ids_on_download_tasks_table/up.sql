DROP INDEX IF EXISTS ux_download_tasks;

ALTER TABLE download_tasks ADD COLUMN new_artist_id VARCHAR(64) DEFAULT NULL;
UPDATE download_tasks SET new_artist_id = CAST(artist_id as VARCHAR(64));
ALTER TABLE download_tasks DROP COLUMN artist_id;
ALTER TABLE download_tasks ADD COLUMN artist_id VARCHAR(64) DEFAULT NULL;
UPDATE download_tasks SET artist_id = new_artist_id;
ALTER TABLE download_tasks DROP COLUMN new_artist_id;

ALTER TABLE download_tasks ADD COLUMN new_album_id VARCHAR(64) DEFAULT NULL;
UPDATE download_tasks SET new_album_id = CAST(album_id as VARCHAR(64));
ALTER TABLE download_tasks DROP COLUMN album_id;
ALTER TABLE download_tasks ADD COLUMN album_id VARCHAR(64) DEFAULT NULL;
UPDATE download_tasks SET album_id = new_album_id;
ALTER TABLE download_tasks DROP COLUMN new_album_id;

ALTER TABLE download_tasks ADD COLUMN new_track_id VARCHAR(64) DEFAULT NULL;
UPDATE download_tasks SET new_track_id = CAST(track_id as VARCHAR(64));
ALTER TABLE download_tasks DROP COLUMN track_id;
ALTER TABLE download_tasks ADD COLUMN track_id VARCHAR(64) DEFAULT NULL;
UPDATE download_tasks SET track_id = new_track_id;
ALTER TABLE download_tasks DROP COLUMN new_track_id;

CREATE UNIQUE INDEX ux_download_tasks ON download_tasks(
    `type`,
    ifnull(`track_id`, 0),
    ifnull(`album_id`, 0),
    ifnull(`source`, ''),
    ifnull(`quality`, ''),
    `file_path`,
    ifnull(`total_bytes`, 0)
);
