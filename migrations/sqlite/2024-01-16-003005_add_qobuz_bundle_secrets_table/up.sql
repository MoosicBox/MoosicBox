CREATE TABLE IF NOT EXISTS qobuz_bundle_secrets (
    `id` INTEGER PRIMARY KEY NOT NULL,
    `qobuz_bundle_id` INTEGER NOT NULL,
    `timezone` VARCHAR(32) NOT NULL,
    `secret` VARCHAR(32) NOT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    FOREIGN KEY (qobuz_bundle_id) REFERENCES qobuz_bundles(id)
);
