CREATE TABLE IF NOT EXISTS download_locations (
    `id` INTEGER PRIMARY KEY NOT NULL,
    `path` TEXT DEFAULT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
CREATE UNIQUE INDEX ux_download_locations ON download_locations(
    ifnull(`path`, '')
);
