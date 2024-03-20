ALTER TABLE artists ADD COLUMN tidal_id INTEGER DEFAULT NULL;
ALTER TABLE albums ADD COLUMN tidal_id INTEGER DEFAULT NULL;
ALTER TABLE tracks ADD COLUMN tidal_id INTEGER DEFAULT NULL;

DROP INDEX IF EXISTS ux_tracks_file;
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    coalesce("file", ''),
    "album_id",
    "title",
    "duration",
    "number",
    coalesce("format", ''),
    "source",
    coalesce("tidal_id", 0)
);
