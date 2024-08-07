CREATE TABLE IF NOT EXISTS audio_zone_players (
    audio_zone_id BIGINT NOT NULL,
    player_id BIGINT NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX ux_audio_zone_players_props ON audio_zone_players(
    audio_zone_id,
    player_id
);
