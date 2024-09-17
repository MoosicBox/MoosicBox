CREATE TABLE IF NOT EXISTS audio_zone_sessions (
    audio_zone_id INTEGER NOT NULL,
    session_id INTEGER NOT NULL,
    created TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);

CREATE UNIQUE INDEX ux_audio_zone_sessions_props ON audio_zone_sessions(
    audio_zone_id,
    session_id
);
