CREATE TABLE IF NOT EXISTS albums (
    id INTEGER PRIMARY KEY NOT NULL AUTO_INCREMENT,
    artist_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    date_released TEXT,
    date_added TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f')),
    artwork TEXT,
    directory TEXT,
    blur INTEGER NOT NULL DEFAULT 0
)
