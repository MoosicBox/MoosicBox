ALTER TABLE tracks ADD COLUMN format VARCHAR(64) NOT NULL DEFAULT 'SOURCE';
UPDATE tracks SET format = 'FLAC' WHERE UPPER(`file`) LIKE '%.FLAC';
UPDATE tracks SET format = 'AAC' WHERE UPPER(`file`) LIKE '%.M4A';
UPDATE tracks SET format = 'MP3' WHERE UPPER(`file`) LIKE '%.MP3';
UPDATE tracks SET format = 'OPUS' WHERE UPPER(`file`) LIKE '%.OPUS';
