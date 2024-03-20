ALTER TABLE session_playlist_tracks ADD COLUMN "type" VARCHAR(64) NOT NULL DEFAULT 'LIBRARY';
ALTER TABLE session_playlist_tracks ADD COLUMN "data" TEXT DEFAULT NULL;

CREATE TABLE IF NOT EXISTS session_playlist_tracks_temp (
    id INTEGER PRIMARY KEY NOT NULL,
    session_playlist_id INTEGER NOT NULL,
    track_id INTEGER DEFAULT NULL,
    "type" VARCHAR(64) NOT NULL DEFAULT 'LIBRARY',
    "data" TEXT DEFAULT NULL
);

INSERT INTO session_playlist_tracks_temp SELECT * FROM session_playlist_tracks;

DROP TABLE IF EXISTS session_playlist_tracks;

CREATE TABLE IF NOT EXISTS session_playlist_tracks (
    id INTEGER PRIMARY KEY NOT NULL,
    session_playlist_id INTEGER NOT NULL,
    track_id INTEGER DEFAULT NULL,
    "type" VARCHAR(64) NOT NULL DEFAULT 'LIBRARY',
    "data" TEXT DEFAULT NULL
);

INSERT INTO session_playlist_tracks SELECT * FROM session_playlist_tracks_temp;

DROP TABLE IF EXISTS session_playlist_tracks_temp;
