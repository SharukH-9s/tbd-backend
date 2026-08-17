-- Migration: 20260101000001_initial_schema.sql
-- Initial database schema for TurfBD backend

-- Users table
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    phone TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'User',
    location TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Refresh tokens table
CREATE TABLE IF NOT EXISTS refresh_tokens (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Turfs table
CREATE TABLE IF NOT EXISTS turfs (
    id SERIAL PRIMARY KEY,
    manager_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    location TEXT NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Games table
CREATE TABLE IF NOT EXISTS games (
    id SERIAL PRIMARY KEY,
    turf_id INTEGER NOT NULL REFERENCES turfs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    game_start_time TIME WITHOUT TIME ZONE NOT NULL DEFAULT '08:00:00',
    game_end_time TIME WITHOUT TIME ZONE NOT NULL DEFAULT '01:00:00'
);

-- Game Settings table (template configurations for recurring slots)
CREATE TABLE IF NOT EXISTS game_settings (
    id SERIAL PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    base_price NUMERIC(10,2) NOT NULL,
    slot_duration_mins INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Slots table
CREATE TABLE IF NOT EXISTS slots (
    id SERIAL PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    settings_id INTEGER REFERENCES game_settings(id),
    label TEXT,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    final_price NUMERIC(10,2) NOT NULL,
    status TEXT NOT NULL DEFAULT 'Available',
    pending_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Bookings table
CREATE TABLE IF NOT EXISTS bookings (
    id SERIAL PRIMARY KEY,
    slot_id INTEGER NOT NULL REFERENCES slots(id) ON DELETE CASCADE,
    user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    contact_name TEXT,
    contact_phone TEXT,
    payment_status TEXT NOT NULL DEFAULT 'Unpaid',
    booking_time TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Outbox table (Transactional Outbox Pattern for async event publishing)
CREATE TABLE IF NOT EXISTS outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    process_after TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Processed Jobs table (Idempotency tracking for consumers)
CREATE TABLE IF NOT EXISTS processed_jobs (
    outbox_id UUID NOT NULL,
    consumer TEXT NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (outbox_id, consumer)
);

-- Performance & Lookup Indexes
CREATE INDEX IF NOT EXISTS idx_bookings_user ON bookings (user_id);
CREATE INDEX IF NOT EXISTS idx_bookings_slot ON bookings (slot_id);
CREATE INDEX IF NOT EXISTS idx_slots_game_time ON slots (game_id, start_time);
CREATE INDEX IF NOT EXISTS idx_slots_pending_expiry ON slots (status, pending_until) WHERE status = 'Pending';
CREATE INDEX IF NOT EXISTS idx_outbox_queued ON outbox (created_at, process_after) WHERE status = 'queued';
