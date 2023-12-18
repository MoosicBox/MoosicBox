DELETE FROM tracks
WHERE rowid NOT IN (
    SELECT MIN(rowid)
    FROM tracks
    GROUP BY `file`
);

CREATE UNIQUE INDEX ux_tracks_file ON tracks(file);
