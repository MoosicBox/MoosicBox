CREATE TABLE IF NOT EXISTS yt_config (
    `id` INTEGER PRIMARY KEY NOT NULL,
    `access_token` VARCHAR(256) NOT NULL,
    `user_id` INTEGER NOT NULL,
    `user_email` VARCHAR(256) NOT NULL,
    `user_public_id` VARCHAR(256) NOT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
