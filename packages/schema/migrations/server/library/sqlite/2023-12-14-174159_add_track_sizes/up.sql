CREATE TABLE IF NOT EXISTS track_sizes (
    id INTEGER PRIMARY KEY NOT NULL,
    track_id INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    format VARCHAR(64) NOT NULL,
    created TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
