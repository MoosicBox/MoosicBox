CREATE TABLE IF NOT EXISTS players (
    id INTEGER PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL,
    name TEXT NOT NULL,
    `type` TEXT NOT NULL,
    playing INTEGER NOT NULL DEFAULT 0,
    created TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    updated TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    FOREIGN KEY (connection_id) REFERENCES connections(id)
);
