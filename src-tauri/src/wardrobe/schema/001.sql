CREATE TABLE storage_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    phase_one_schema INTEGER NOT NULL CHECK (phase_one_schema = 1),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE garment_categories (
    id TEXT PRIMARY KEY,
    group_name TEXT NOT NULL CHECK (group_name IN ('Upper wear', 'Lower wear', 'Outerwear', 'Accessories')),
    label TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE garments (
    id TEXT PRIMARY KEY CHECK (length(id) = 32),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    category_id TEXT NOT NULL REFERENCES garment_categories(id),
    added_at INTEGER NOT NULL,
    changed_at INTEGER NOT NULL
) STRICT;

CREATE TABLE garment_metadata (
    garment_id TEXT PRIMARY KEY REFERENCES garments(id) ON DELETE CASCADE,
    primary_colour TEXT NOT NULL,
    secondary_colours TEXT NOT NULL CHECK (json_valid(secondary_colours) AND json_type(secondary_colours) = 'array'),
    pattern TEXT NOT NULL,
    fit TEXT NOT NULL,
    sleeve TEXT NOT NULL,
    fabric TEXT NOT NULL,
    formality TEXT NOT NULL,
    seasons TEXT NOT NULL CHECK (json_valid(seasons) AND json_type(seasons) = 'array'),
    notes TEXT NOT NULL
) STRICT;

CREATE TABLE garment_images (
    id TEXT PRIMARY KEY CHECK (length(id) = 32),
    garment_id TEXT NOT NULL UNIQUE REFERENCES garments(id) ON DELETE CASCADE
) STRICT;

CREATE TABLE image_assets (
    image_id TEXT NOT NULL REFERENCES garment_images(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('original','display','thumbnail')),
    relative_path TEXT NOT NULL UNIQUE,
    mime TEXT NOT NULL CHECK (mime IN ('image/jpeg','image/png')),
    width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 6000),
    height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 6000),
    byte_count INTEGER NOT NULL CHECK (byte_count BETWEEN 1 AND 12582912),
    sha256 BLOB NOT NULL CHECK (length(sha256) = 32),
    processing_version INTEGER NOT NULL CHECK (processing_version > 0),
    PRIMARY KEY (image_id, kind)
) STRICT;
