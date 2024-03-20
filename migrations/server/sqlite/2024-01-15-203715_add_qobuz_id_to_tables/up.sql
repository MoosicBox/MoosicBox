ALTER TABLE artists ADD COLUMN qobuz_id INTEGER DEFAULT NULL;
ALTER TABLE albums ADD COLUMN qobuz_id INTEGER DEFAULT NULL;
ALTER TABLE tracks ADD COLUMN qobuz_id INTEGER DEFAULT NULL;

DROP INDEX IF EXISTS ux_tracks_file;
DELETE FROM tracks
WHERE rowid NOT IN (
    SELECT MIN(rowid)
    FROM tracks
    GROUP BY
        ifnull(`file`, ''),
        `album_id`,
        `title`,
        `duration`,
        `number`,
        ifnull(`format`, ''),
        `source`,
        ifnull(`tidal_id`, 0),
        ifnull(`qobuz_id`, 0)
);
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    ifnull(`file`, ''),
    `album_id`,
    `title`,
    `duration`,
    `number`,
    ifnull(`format`, ''),
    `source`,
    ifnull(`tidal_id`, 0),
    ifnull(`qobuz_id`, 0)
);
