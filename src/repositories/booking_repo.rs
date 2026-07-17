use crate::{db_conn_err, db_err};
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::bookings::Booking;
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
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "INSERT INTO bookings (slot_id, user_id, contact_name, contact_phone, payment_status)
             VALUES ($1, $2, $3, $4, 'Unpaid')
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status, booking_time",
            slot_id,
            user_id,
            contact_name,
            contact_phone,
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
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "INSERT INTO bookings (slot_id, user_id, contact_name, contact_phone, payment_status)
             VALUES ($1, NULL, $2, $3, 'Fully_Paid')
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status, booking_time",
            slot_id,
            contact_name,
            contact_phone,
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
            "SELECT id, slot_id, user_id, contact_name, contact_phone, payment_status, booking_time
             FROM bookings WHERE id = $1",
            booking_id,
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

    /// Updates payment_status for a booking by its own ID.
    async fn update_by_booking_id_tx(
        executor: &mut sqlx::PgConnection,
        status: &str,
        booking_id: i32,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "UPDATE bookings SET payment_status = $1 WHERE id = $2
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status, booking_time",
            status,
            booking_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to update payment status"))
    }

    /// Updates payment_status for a booking identified by (slot_id, user_id).
    async fn update_by_slot_and_user_tx(
        executor: &mut sqlx::PgConnection,
        status: &str,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, AppError> {
        sqlx::query_as!(
            Booking,
            "UPDATE bookings SET payment_status = $1 WHERE slot_id = $2 AND user_id = $3
             RETURNING id, slot_id, user_id, contact_name, contact_phone, payment_status, booking_time",
            status,
            slot_id,
            user_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to update payment status"))
    }

    /// Updates payment_status for a booking inside a transaction.
    /// Can filter by booking_id OR (slot_id AND user_id).
    pub async fn update_payment_status_tx(
        executor: &mut sqlx::PgConnection,
        status: &str,
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
        Self::update_payment_status_tx(executor, "Fully_Paid", Some(booking_id), None, None).await
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
            "SELECT b.id, b.slot_id, b.payment_status, b.booking_time,
                    b.contact_name, b.contact_phone,
                    t.name as turf_name, g.name as game_name,
                    s.final_price, s.start_time, s.end_time,
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
                let total_price: BigDecimal = row.final_price;
                let payment_status: String = row.payment_status;

                let (advance_paid, due_amount) = match payment_status.as_str() {
                    "Unpaid" => (BigDecimal::from(0), total_price.clone()),
                    "Partially_Paid" => {
                        let advance = &total_price * BigDecimal::from(30) / BigDecimal::from(100);
                        let due = &total_price * BigDecimal::from(70) / BigDecimal::from(100);
                        (advance, due)
                    }
                    "Fully_Paid" => (total_price.clone(), BigDecimal::from(0)),
                    _ => (BigDecimal::from(0), total_price.clone()),
                };

                // Prefer the joined user profile; fall back to the contact fields stored at
                // booking time for offline/unauthenticated bookings.
                let user_name = row.username.or(row.contact_name);
                let user_phone = row.phone.or(row.contact_phone);

                BookingDetail {
                    id: row.id,
                    slot_id: row.slot_id,
                    booking_number: row.id,
                    user_name,
                    user_phone,
                    turf_name: row.turf_name,
                    game_name: row.game_name,
                    total_price,
                    advance_paid,
                    due_amount,
                    payment_status,
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
            "SELECT b.id, b.slot_id, b.payment_status, b.booking_time,
                    b.contact_name, b.contact_phone,
                    t.name as turf_name, g.name as game_name,
                    s.final_price, s.start_time, s.end_time,
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
                let total_price: BigDecimal = row.final_price;
                let payment_status: String = row.payment_status;

                let (advance_paid, due_amount) = match payment_status.as_str() {
                    "Unpaid" => (BigDecimal::from(0), total_price.clone()),
                    "Partially_Paid" => {
                        let advance = &total_price * BigDecimal::from(30) / BigDecimal::from(100);
                        let due = &total_price * BigDecimal::from(70) / BigDecimal::from(100);
                        (advance, due)
                    }
                    "Fully_Paid" => (total_price.clone(), BigDecimal::from(0)),
                    _ => (BigDecimal::from(0), total_price.clone()),
                };

                let user_name = row.username.or(row.contact_name);
                let user_phone = row.phone.or(row.contact_phone);

                BookingDetail {
                    id: row.id,
                    slot_id: row.slot_id,
                    booking_number: row.id,
                    user_name,
                    user_phone,
                    turf_name: row.turf_name,
                    game_name: row.game_name,
                    total_price,
                    advance_paid,
                    due_amount,
                    payment_status,
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
