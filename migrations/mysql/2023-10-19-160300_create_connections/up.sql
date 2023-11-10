CREATE TABLE IF NOT EXISTS connections (
    id VARCHAR(256) PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f')),
    updated TEXT DEFAULT (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%M:%f'))
);
