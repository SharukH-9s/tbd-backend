-- Enforce the invariant that each slot can have at most one booking.
ALTER TABLE public.bookings
ADD CONSTRAINT bookings_slot_id_unique UNIQUE (slot_id);
