-- Turf Booking System Schema

-- Enum for Slot Status (if not already handled via text check)
-- CREATE TYPE slot_status AS ENUM ('Available', 'Pending', 'Booked');

CREATE TABLE turfs (
    id SERIAL PRIMARY KEY,
    manager_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    location TEXT NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE games (
    id SERIAL PRIMARY KEY,
    turf_id INTEGER NOT NULL REFERENCES turfs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,             -- "Football", "Cricket", "Badminton"
    price_per_hour DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- MODIFIED: slots now links to a game, not directly to a turf
CREATE TABLE slots (
    id SERIAL PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE, -- changed!
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'Available',
    pending_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE bookings (
    id SERIAL PRIMARY KEY,
    slot_id INTEGER NOT NULL REFERENCES slots(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    payment_status TEXT NOT NULL DEFAULT 'Unpaid', -- 'Unpaid', 'Paid'
    booking_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for searching slots by turf and time
CREATE INDEX idx_slots_turf_time ON slots(turf_id, start_time);
-- Index for background worker to find expired pending slots
CREATE INDEX idx_slots_pending_expiry ON slots(status, pending_until) WHERE status = 'Pending';
