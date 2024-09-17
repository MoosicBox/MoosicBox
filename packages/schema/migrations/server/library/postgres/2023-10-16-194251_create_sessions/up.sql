CREATE TABLE IF NOT EXISTS session_playlists (
    id BIGSERIAL PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS session_playlist_tracks (
    id BIGSERIAL PRIMARY KEY NOT NULL,
    session_playlist_id BIGINT NOT NULL,
    track_id BIGINT NOT NULL,
    FOREIGN KEY (session_playlist_id) REFERENCES session_playlists(id),
    FOREIGN KEY (track_id) REFERENCES tracks(id)
);

CREATE TABLE IF NOT EXISTS sessions (
    id BIGSERIAL PRIMARY KEY NOT NULL,
    session_playlist_id BIGINT NOT NULL,
    name TEXT NOT NULL,
    active BIGINT NOT NULL DEFAULT 0,
    playing BIGINT NOT NULL DEFAULT 0,
    position BIGINT,
    seek BIGINT,
    FOREIGN KEY (session_playlist_id) REFERENCES session_playlists(id)
);
