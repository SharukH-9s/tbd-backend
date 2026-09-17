-- Migration: Add turf_photos table for photo gallery feature
-- Uses PostgreSQL BYTEA for storing image binary data directly.
-- game_id is nullable: NULL means the "Washroom and Changing" (facilities) section.
-- ON DELETE CASCADE on turf_id and game_id ensures photo rows are removed automatically when a turf or game is deleted.

CREATE TABLE IF NOT EXISTS public.turf_photos (
    id          SERIAL PRIMARY KEY,
    turf_id     INTEGER  NOT NULL REFERENCES public.turfs(id) ON DELETE CASCADE,
    game_id     INTEGER  REFERENCES public.games(id) ON DELETE CASCADE,  -- NULL for "Washroom and Changing"
    category    TEXT     NOT NULL,                                         -- Game name or "Washroom and Changing"
    filename    TEXT     NOT NULL,                                         -- Original uploaded filename
    mime_type   TEXT     NOT NULL,                                         -- e.g. "image/jpeg", "image/png", "image/webp"
    file_size   INTEGER  NOT NULL,                                         -- Size in bytes (<= 1048576)
    photo_data  BYTEA    NOT NULL,                                         -- Raw binary image data
    created_at  TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_turf_photos_turf     ON public.turf_photos(turf_id);
CREATE INDEX IF NOT EXISTS idx_turf_photos_game     ON public.turf_photos(game_id);
CREATE INDEX IF NOT EXISTS idx_turf_photos_category ON public.turf_photos(turf_id, category);
