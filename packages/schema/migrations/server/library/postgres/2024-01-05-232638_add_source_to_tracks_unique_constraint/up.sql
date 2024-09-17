DROP INDEX IF EXISTS ux_tracks_file;
CREATE UNIQUE INDEX ux_tracks_file ON tracks(
    coalesce("file", ''),
    "album_id",
    "title",
    "duration",
    "number",
    coalesce("format", ''),
    "source"
);
