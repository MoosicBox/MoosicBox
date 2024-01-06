ALTER TABLE artists DROP COLUMN tidal_id;
ALTER TABLE albums DROP COLUMN tidal_id;

DROP INDEX IF EXISTS ux_tracks_file;
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    ifnull(`file`, ''),
    `album_id`,
    `title`,
    `duration`,
    `number`,
    ifnull(`format`, '')
);

ALTER TABLE tracks DROP COLUMN tidal_id;
