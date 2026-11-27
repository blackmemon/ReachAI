ALTER TABLE conversations ADD COLUMN timer_extended boolean NOT NULL DEFAULT false;
ALTER TABLE sources ADD COLUMN pause_count integer NOT NULL DEFAULT 0;
-- Place lookups are public data, shared by all companies (no tenant column).
CREATE TABLE geo_cache (
    query text PRIMARY KEY,
    south double precision NOT NULL,
    west double precision NOT NULL,
    north double precision NOT NULL,
    east double precision NOT NULL,
    cached_at timestamptz NOT NULL DEFAULT now()
);
