CREATE TABLE IF NOT EXISTS players (
    id INTEGER PRIMARY KEY NOT NULL AUTO_INCREMENT,
    connection_id TEXT NOT NULL,
    name TEXT NOT NULL,
    `type` TEXT NOT NULL,
    playing INTEGER NOT NULL DEFAULT 0,
    created TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f')),
    updated TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f'))
);
