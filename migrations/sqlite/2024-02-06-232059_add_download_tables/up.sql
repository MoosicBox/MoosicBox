CREATE TABLE IF NOT EXISTS download_tasks (
    `id` INTEGER PRIMARY KEY NOT NULL,
    `state` VARCHAR(32) NOT NULL DEFAULT 'PENDING',
    `type` VARCHAR(32) NOT NULL,
    `track_id` INTEGER DEFAULT NULL,
    `album_id` INTEGER DEFAULT NULL,
    `source` VARCHAR(32) DEFAULT NULL,
    `quality` VARCHAR(32) DEFAULT NULL,
    `file_path` VARCHAR(1024) NOT NULL,
    `progress` REAL NOT NULL DEFAULT 0,
    `bytes` INTEGER NOT NULL DEFAULT 0,
    `speed` INTEGER DEFAULT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);

CREATE UNIQUE INDEX ux_download_tasks ON download_tasks(
    `type`,
    ifnull(`track_id`, 0),
    ifnull(`album_id`, 0),
    ifnull(`source`, ''),
    ifnull(`quality`, ''),
    `file_path`
);
