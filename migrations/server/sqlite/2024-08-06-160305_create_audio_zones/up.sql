CREATE TABLE IF NOT EXISTS audio_zones (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
