ALTER TABLE artists ADD COLUMN qobuz_id BIGINT DEFAULT NULL;
ALTER TABLE albums ADD COLUMN qobuz_id BIGINT DEFAULT NULL;
ALTER TABLE tracks ADD COLUMN qobuz_id BIGINT DEFAULT NULL;

DROP INDEX IF EXISTS ux_tracks_file;
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    coalesce("file", ''),
    "album_id",
    "title",
    "duration",
    "number",
    coalesce("format", ''),
    "source",
    coalesce("tidal_id", 0),
    coalesce("qobuz_id", 0)
);
