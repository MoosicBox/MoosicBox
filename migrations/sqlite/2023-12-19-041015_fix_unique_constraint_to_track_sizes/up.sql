DROP INDEX IF EXISTS ux_track_sizes_props;
CREATE UNIQUE INDEX ux_track_sizes_props ON track_sizes(
    track_id,
    ifnull(`format`, ''),
    ifnull(`audio_bitrate`, 0),
    ifnull(`overall_bitrate`, 0),
    ifnull(`bit_depth`, 0),
    ifnull(`sample_rate`, 0),
    ifnull(`channels`, 0)
);
