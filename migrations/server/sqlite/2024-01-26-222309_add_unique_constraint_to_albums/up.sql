DROP INDEX IF EXISTS ux_albums;
DELETE FROM albums
WHERE rowid NOT IN (
    SELECT MIN(rowid)
    FROM albums
    GROUP BY
        `artist_id`,
        `title`,
        ifnull(`directory`, ''),
        ifnull(`tidal_id`, 0),
        ifnull(`qobuz_id`, '')
);
CREATE UNIQUE INDEX ux_albums ON albums(
    `artist_id`,
    `title`,
    ifnull(`directory`, ''),
    ifnull(`tidal_id`, 0),
    ifnull(`qobuz_id`, '')
);
