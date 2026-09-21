-- Migration: Enforce at most 1 "Turf Cover" photo per turf at the DB level.
-- A partial unique index on (turf_id) WHERE category = 'Turf Cover' guarantees
-- uniqueness without affecting any other category rows.

CREATE UNIQUE INDEX IF NOT EXISTS idx_turf_photos_cover_unique
    ON public.turf_photos (turf_id)
    WHERE category = 'Turf Cover';
