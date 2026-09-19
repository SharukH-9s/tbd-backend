-- Add payment amount columns to bookings table
ALTER TABLE bookings
  ADD COLUMN total_price   NUMERIC(10,2) NOT NULL,
  ADD COLUMN paid_amount   NUMERIC(10,2) NOT NULL DEFAULT 0,
  ADD COLUMN unpaid_amount NUMERIC(10,2) NOT NULL DEFAULT 0;

-- Integrity constraints for monetary values
ALTER TABLE bookings
  ADD CONSTRAINT chk_total_price_non_negative   CHECK (total_price >= 0),
  ADD CONSTRAINT chk_paid_amount_non_negative   CHECK (paid_amount >= 0),
  ADD CONSTRAINT chk_unpaid_amount_non_negative CHECK (unpaid_amount >= 0),
  ADD CONSTRAINT chk_paid_lte_total             CHECK (paid_amount <= total_price),
  ADD CONSTRAINT chk_balance_identity           CHECK (unpaid_amount = total_price - paid_amount);
