CREATE TABLE IF NOT EXISTS audio_outputs (
    `id` VARCHAR(128) PRIMARY KEY NOT NULL,
    `name` VARCHAR(256) NOT NULL,
    `spec_rate` INTEGER NOT NULL,
    `spec_channels` INTEGER NOT NULL,
    `created` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
    `updated` TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now'))
);
