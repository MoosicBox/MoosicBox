CREATE TABLE IF NOT EXISTS audio_zone_sessions (
    audio_zone_id BIGINT NOT NULL,
    session_id BIGINT NOT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX ux_audio_zone_sessions_props ON audio_zone_sessions(
    audio_zone_id,
    session_id
);
