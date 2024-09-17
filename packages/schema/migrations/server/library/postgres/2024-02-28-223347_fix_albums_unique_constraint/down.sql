DROP INDEX IF EXISTS ux_albums;
CREATE UNIQUE INDEX ux_albums ON albums(
    "artist_id",
    "title",
    coalesce("directory", ''),
    coalesce("tidal_id", 0),
    coalesce("qobuz_id", '')
);
