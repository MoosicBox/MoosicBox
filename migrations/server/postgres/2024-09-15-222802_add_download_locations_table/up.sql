CREATE TABLE IF NOT EXISTS download_locations (
    "id" BIGSERIAL PRIMARY KEY NOT NULL,
    "path" TEXT DEFAULT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX ux_download_locations ON download_locations(
    coalesce("path", '')
);
