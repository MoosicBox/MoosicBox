CREATE TABLE IF NOT EXISTS session_playlists (
    id INTEGER PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS session_playlist_tracks (
    id INTEGER PRIMARY KEY NOT NULL,
    session_playlist_id INTEGER NOT NULL,
    track_id INTEGER NOT NULL,
    FOREIGN KEY (session_playlist_id) REFERENCES session_playlist(id),
    FOREIGN KEY (track_id) REFERENCES tracks(id)
);

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY NOT NULL,
    session_playlist_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    playing INTEGER NOT NULL DEFAULT 0,
    position INTEGER,
    seek INTEGER,
    FOREIGN KEY (session_playlist_id) REFERENCES session_playlist(id)
);
