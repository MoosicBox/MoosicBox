CREATE UNIQUE INDEX ux_track_sizes_props ON track_sizes(
    track_id,
    format,
    audio_bitrate,
    overall_bitrate,
    bit_depth,
    sample_rate,
    channels
);
