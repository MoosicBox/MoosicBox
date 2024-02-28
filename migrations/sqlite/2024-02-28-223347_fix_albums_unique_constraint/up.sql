DROP INDEX IF EXISTS ux_albums;
DELETE FROM albums
WHERE rowid NOT IN (
    SELECT MIN(rowid)
    FROM albums
    GROUP BY
        `artist_id`,
        `title`
);
CREATE UNIQUE INDEX ux_albums ON albums(
    `artist_id`,
    `title`
);
