CREATE TABLE IF NOT EXISTS connections (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
