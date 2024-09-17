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
        `source`
);
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    ifnull(`file`, ''),
    `album_id`,
    `title`,
    `duration`,
    `number`,
    ifnull(`format`, ''),
    `source`
);
