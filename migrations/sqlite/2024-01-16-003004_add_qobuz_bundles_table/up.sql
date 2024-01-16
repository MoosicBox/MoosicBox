CREATE TABLE IF NOT EXISTS qobuz_bundles (
    `id` INTEGER PRIMARY KEY NOT NULL,
    `bundle_version` VARCHAR(32) NOT NULL,
    `app_id` VARCHAR(32) NOT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
