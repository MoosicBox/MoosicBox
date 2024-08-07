CREATE TABLE IF NOT EXISTS audio_zone_players (
    audio_zone_id INTEGER NOT NULL,
    player_id INTEGER NOT NULL,
    created TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);

CREATE UNIQUE INDEX ux_audio_zone_players_props ON audio_zone_players(
    audio_zone_id,
    player_id
);
