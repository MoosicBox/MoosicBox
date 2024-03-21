CREATE TABLE IF NOT EXISTS scan_locations (
    "id" BIGSERIAL PRIMARY KEY NOT NULL,
    "origin" VARCHAR(128) NOT NULL,
    "path" TEXT DEFAULT NULL,
    created TIMESTAMP NOT NULL DEFAULT NOW(),
    updated TIMESTAMP NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX ux_scan_locations ON scan_locations(
    origin,
    coalesce("path", '')
);
