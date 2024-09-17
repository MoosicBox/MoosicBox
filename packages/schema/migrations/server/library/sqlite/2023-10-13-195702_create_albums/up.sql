CREATE TABLE IF NOT EXISTS albums (
    id INTEGER PRIMARY KEY NOT NULL,
    artist_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    date_released TEXT,
    date_added TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f','now')),
    artwork TEXT,
    directory TEXT,
    blur INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (artist_id) REFERENCES artists(id)
)
