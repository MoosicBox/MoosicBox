DROP INDEX IF EXISTS ux_track_sizes_props;
CREATE UNIQUE INDEX ux_track_sizes_props ON track_sizes(
    track_id,
    coalesce("format", ''),
    coalesce("audio_bitrate", 0),
    coalesce("overall_bitrate", 0),
    coalesce("bit_depth", 0),
    coalesce("sample_rate", 0),
    coalesce("channels", 0)
);
