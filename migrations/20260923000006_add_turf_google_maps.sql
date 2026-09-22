-- Add Google Maps location fields to turfs table
ALTER TABLE turfs
    ADD COLUMN IF NOT EXISTS google_maps_url TEXT,
    ADD COLUMN IF NOT EXISTS latitude        DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude       DOUBLE PRECISION;
