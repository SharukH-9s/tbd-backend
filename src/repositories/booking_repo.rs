use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::bookings::{Booking, PaymentStatus};
use crate::{db_conn_err, db_err};
use axum::http::StatusCode;
use bigdecimal::BigDecimal;

pub struct BookingRepository;

impl BookingRepository {
    /// Checks how many active Pending slots a user is currently holding.
    /// Returns true if the user already holds 2 or more — blocks further booking.
    /// A user may hold at most 2 Pending slots simultaneously.
    pub async fn user_has_pending_booking_tx(
        executor: &mut sqlx::PgConnection,
        user_id: i32,
    ) -> Result<bool, AppError> {
        // query_scalar! replaces the (i64,) tuple pattern, returning COUNT(*) directly.
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM bookings b
             JOIN slots s ON b.slot_id = s.id
             WHERE b.user_id = $1
               AND s.status = 'Pending'
               AND s.pending_until > NOW()",
            user_id,
        )
        .fetch_one(executor)
        .await
        .map(|n| n.unwrap_or(0))
        .map_err(db_err!("Failed to check for existing pending bookings"))?;

        Ok(count >= 2) // block if user already holds 2 or more pending slots
    }

    /// Inserts a new booking record inside a transaction.
    /// Called during Phase 1 (initiate booking) with payment_status = 'Unpaid'.
    /// Automatically saves the user's contact name and phone from their profile.
    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
        user_id: i32,
        contact_name: &str,
        contact_phone: &str,
        total_price: &BigDecimal,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "INSERT INTO bookings (slot_id, user_id, contact_name, contact_phone, payment_status, total_price, paid_amount, unpaid_amount)
             VALUES ($1, $2, $3, $4, 'Unpaid', $5, 0, $5)
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time",
            slot_id,
            user_id,
            contact_name,
            contact_phone,
            total_price,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to create booking record"))
    }

    /// Inserts a new offline booking record inside a transaction.
    /// Called when a manager books a slot on behalf of an unregistered user.
    pub async fn insert_offline_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
        contact_name: &str,
        contact_phone: &str,
        total_price: &BigDecimal,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "INSERT INTO bookings (slot_id, user_id, contact_name, contact_phone, payment_status, total_price, paid_amount, unpaid_amount)
             VALUES ($1, NULL, $2, $3, 'Fully_Paid', $4, $4, 0)
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time",
            slot_id,
            contact_name,
            contact_phone,
            total_price,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to record offline booking"))
    }

    /// Fetches a single booking by ID inside a transaction.
    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        booking_id: i32,
    ) -> Result<Option<Booking>, AppError> {
        sqlx::query_as!(
            Booking,
            "SELECT id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time
             FROM bookings WHERE id = $1",
            booking_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(db_err!("Failed to fetch booking from database"))
    }

    /// Fetches a single booking by slot_id and user_id inside a transaction.
    /// Used by the payment gateway flow to validate state before processing payment.
    pub async fn find_by_slot_and_user_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Option<Booking>, AppError> {
        sqlx::query_as!(
            Booking,
            "SELECT id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time
             FROM bookings WHERE slot_id = $1 AND user_id = $2",
            slot_id,
            user_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(db_err!("Failed to fetch booking from database"))
    }

    // ---------------------------------------------------------------------------
    // Pattern 3: update_payment_status_tx refactoring
    //
    // The original function selected one of two SQL strings at runtime based on
    // the arguments passed, which is incompatible with sqlx::query_as!.
    // It is now split into two private concrete helpers, with the original public
    // method preserved as a thin dispatcher.
    // ---------------------------------------------------------------------------

    /// Updates payment_status for a booking by its own ID and marks as fully paid.
    async fn update_by_booking_id_tx(
        executor: &mut sqlx::PgConnection,
        status: PaymentStatus,
        booking_id: i32,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "UPDATE bookings
             SET payment_status = $1,
                 paid_amount = total_price,
                 unpaid_amount = 0
             WHERE id = $2
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time",
            status as PaymentStatus,
            booking_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to update payment status"))
    }

    /// Updates payment_status for a booking identified by (slot_id, user_id) and marks as fully paid.
    async fn update_by_slot_and_user_tx(
        executor: &mut sqlx::PgConnection,
        status: PaymentStatus,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "UPDATE bookings
             SET payment_status = $1,
                 paid_amount = total_price,
                 unpaid_amount = 0
             WHERE slot_id = $2 AND user_id = $3
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time",
            status as PaymentStatus,
            slot_id,
            user_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to update payment status"))
    }

    /// Records advance payment (30%) for a booking identified by (slot_id, user_id).
    pub async fn mark_partially_paid_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "UPDATE bookings
             SET payment_status = 'Partially_Paid',
                 paid_amount = total_price * 30 / 100,
                 unpaid_amount = total_price * 70 / 100
             WHERE slot_id = $1 AND user_id = $2
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status as \"payment_status: _\", total_price, paid_amount, unpaid_amount, booking_time",
            slot_id,
            user_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to record advance payment"))
    }

    /// Updates payment_status for a booking inside a transaction.
    /// Can filter by booking_id OR (slot_id AND user_id).
    pub async fn update_payment_status_tx(
        executor: &mut sqlx::PgConnection,
        status: PaymentStatus,
        booking_id: Option<i32>,
        slot_id: Option<i32>,
        user_id: Option<i32>,
    ) -> Result<Booking, AppError> {
        match (booking_id, slot_id, user_id) {
            (Some(id), _, _) => Self::update_by_booking_id_tx(executor, status, id).await,
            (_, Some(s_id), Some(u_id)) => {
                Self::update_by_slot_and_user_tx(executor, status, s_id, u_id).await
            }
            _ => Err(AppError {
                code: error_codes::INTERNAL_ERROR,
                message: "Invalid parameters for payment status update",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }),
        }
    }

    pub async fn mark_fully_paid_by_manager_tx(
        executor: &mut sqlx::PgConnection,
        booking_id: i32,
    ) -> Result<Booking, AppError> {
        Self::update_payment_status_tx(
            executor,
            PaymentStatus::FullyPaid,
            Some(booking_id),
            None,
            None,
        )
        .await
    }

    // ---------------------------------------------------------------------------
    // Pattern 2: find_enriched_internal refactoring
    //
    // The original generic helper injected a filter clause via format!() at runtime,
    // which is incompatible with sqlx::query!. It has been deleted and replaced by
    // two concrete public functions, each with its own fully-spelled-out SQL literal.
    // The row-mapping logic (advance_paid / due_amount calculation) is preserved.
    // ---------------------------------------------------------------------------

    /// Fetches all bookings for a user with turf, game, and price details.
    pub async fn find_by_user_id_enriched(
        state: &crate::config::state::SharedState,
        user_id: i32,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError>
    {
        use crate::api::handlers::user_handlers::get_user_bookings::BookingDetail;

        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;

        let rows = sqlx::query!(
            // username and phone are NOT NULL in the users table, but come via LEFT JOIN
            // (offline bookings have no user row). The "col?" suffix forces sqlx to
            // infer Option<String> regardless of the underlying column constraint.
            "SELECT b.id, b.slot_id, b.payment_status as \"payment_status: PaymentStatus\", b.booking_time,
                    b.contact_name, b.contact_phone,
                    b.total_price, b.paid_amount, b.unpaid_amount,
                    t.name as turf_name, g.name as game_name,
                    s.start_time, s.end_time,
                    u.username as \"username?\", u.phone as \"phone?\"
             FROM bookings b
             JOIN slots s ON b.slot_id = s.id
             JOIN games g ON s.game_id = g.id
             JOIN turfs t ON g.turf_id = t.id
             LEFT JOIN users u ON b.user_id = u.id
             WHERE b.user_id = $1
             ORDER BY b.booking_time DESC",
            user_id,
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(db_err!("Failed to fetch bookings"))?;

        let booking_details: Vec<BookingDetail> = rows
            .into_iter()
            .map(|row| {
                // Prefer the joined user profile; fall back to the contact fields stored at
                // booking time for offline/unauthenticated bookings.
                let user_name = row.username.or(row.contact_name);
                let user_phone = row.phone.or(row.contact_phone);

                BookingDetail {
                    id: row.id,
                    slot_id: row.slot_id,
                    user_name,
                    user_phone,
                    turf_name: row.turf_name,
                    game_name: row.game_name,
                    total_price: row.total_price,
                    advance_paid: row.paid_amount,
                    due_amount: row.unpaid_amount,
                    payment_status: row.payment_status,
                    booking_time: row.booking_time,
                    start_time: row.start_time,
                    end_time: row.end_time,
                }
            })
            .collect();

        Ok(booking_details)
    }

    /// Fetches all bookings for a specific game with turf, game, and price details.
    /// Used by managers and admins to see all bookings for a specific game.
    pub async fn find_by_game_id_enriched(
        state: &crate::config::state::SharedState,
        game_id: i32,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError>
    {
        use crate::api::handlers::user_handlers::get_user_bookings::BookingDetail;

        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;

        let rows = sqlx::query!(
            "SELECT b.id, b.slot_id, b.payment_status as \"payment_status: PaymentStatus\", b.booking_time,
                    b.contact_name, b.contact_phone,
                    b.total_price, b.paid_amount, b.unpaid_amount,
                    t.name as turf_name, g.name as game_name,
                    s.start_time, s.end_time,
                    u.username as \"username?\", u.phone as \"phone?\"
             FROM bookings b
             JOIN slots s ON b.slot_id = s.id
             JOIN games g ON s.game_id = g.id
             JOIN turfs t ON g.turf_id = t.id
             LEFT JOIN users u ON b.user_id = u.id
             WHERE g.id = $1
             ORDER BY b.booking_time DESC",
            game_id,
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(db_err!("Failed to fetch bookings"))?;

        let booking_details: Vec<BookingDetail> = rows
            .into_iter()
            .map(|row| {
                let user_name = row.username.or(row.contact_name);
                let user_phone = row.phone.or(row.contact_phone);

                BookingDetail {
                    id: row.id,
                    slot_id: row.slot_id,
                    user_name,
                    user_phone,
                    turf_name: row.turf_name,
                    game_name: row.game_name,
                    total_price: row.total_price,
                    advance_paid: row.paid_amount,
                    due_amount: row.unpaid_amount,
                    payment_status: row.payment_status,
                    booking_time: row.booking_time,
                    start_time: row.start_time,
                    end_time: row.end_time,
                }
            })
            .collect();

        Ok(booking_details)
    }

    /// Deletes orphaned 'Unpaid' booking records whose slot is now 'Available'.
    /// This happens when a user initiates a booking but never pays within 15 minutes.
    /// The slot is first reset to 'Available' by the slot cleanup query, and then
    /// this query removes the dangling booking row that was left behind.
    /// Called by the background cleanup worker every 60 seconds, after slot reset.
    pub async fn delete_orphaned_unpaid_bookings(db: &sqlx::PgPool) -> Result<u64, AppError> {
        sqlx::query!(
            "DELETE FROM bookings
             WHERE payment_status = 'Unpaid'
               AND slot_id IN (
                   SELECT id FROM slots WHERE status = 'Available'
               )",
        )
        .execute(db)
        .await
        .map(|result| result.rows_affected())
        .map_err(db_err!("Failed to clean up orphaned booking records"))
    }

    /// Deletes a booking by its own ID inside a transaction.
    /// Used by cancel_booking — the slot reset is handled separately.
    pub async fn delete_by_id_tx(
        executor: &mut sqlx::PgConnection,
        booking_id: i32,
    ) -> Result<(), AppError> {
        sqlx::query!("DELETE FROM bookings WHERE id = $1", booking_id)
            .execute(executor)
            .await
            .map_err(db_err!("Failed to delete booking"))?;
        Ok(())
    }
}
