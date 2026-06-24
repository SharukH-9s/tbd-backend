use crate::{db_conn_err, db_err};
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::bookings::Booking;
use axum::http::StatusCode;
use sqlx::Row;
use sqlx::query_as;

pub struct BookingRepository;

impl BookingRepository {
    /// Checks how many active Pending slots a user is currently holding.
    /// Returns true if the user already holds 2 or more — blocks further booking.
    /// A user may hold at most 2 Pending slots simultaneously.
    pub async fn user_has_pending_booking_tx(
        executor: &mut sqlx::PgConnection,
        user_id: i32,
    ) -> Result<bool, AppError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)
             FROM bookings b
             JOIN slots s ON b.slot_id = s.id
             WHERE b.user_id = $1
               AND s.status = 'Pending'
               AND s.pending_until > NOW()",
        )
        .bind(user_id)
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to check for existing pending bookings"))?;

        Ok(row.0 >= 2) // block if user already holds 2 or more pending slots
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
        query_as::<_, Booking>(
            "INSERT INTO bookings (slot_id, user_id, contact_name, contact_phone, payment_status)
             VALUES ($1, $2, $3, $4, 'Unpaid')
             RETURNING *",
        )
        .bind(slot_id)
        .bind(user_id)
        .bind(contact_name)
        .bind(contact_phone)
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
        query_as::<_, Booking>(
            "INSERT INTO bookings (slot_id, user_id, contact_name, contact_phone, payment_status)
             VALUES ($1, NULL, $2, $3, 'Fully_Paid')
             RETURNING *",
        )
        .bind(slot_id)
        .bind(contact_name)
        .bind(contact_phone)
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to record offline booking"))
    }

    /// Fetches a single booking by ID inside a transaction.
    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        booking_id: i32,
    ) -> Result<Option<Booking>, AppError> {
        query_as::<_, Booking>("SELECT * FROM bookings WHERE id = $1")
            .bind(booking_id)
            .fetch_optional(executor)
            .await
            .map_err(db_err!("Failed to fetch booking from database"))
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
        let (query, filter_val1, filter_val2) = match (booking_id, slot_id, user_id) {
            (Some(id), _, _) => (
                "UPDATE bookings SET payment_status = $1 WHERE id = $2 RETURNING *",
                id,
                0, // unused
            ),
            (_, Some(s_id), Some(u_id)) => (
                "UPDATE bookings SET payment_status = $1 WHERE slot_id = $2 AND user_id = $3 RETURNING *",
                s_id,
                u_id,
            ),
            _ => {
                return Err(AppError {
                    code: error_codes::INTERNAL_ERROR,
                    message: "Invalid parameters for payment status update",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                });
            }
        };

        // building the query object and attaching (binding) all the necessary parameters
        let mut q = sqlx::query_as::<_, Booking>(query)
            .bind(status) // $1: "Partially_Paid" or "Fully_Paid"
            .bind(filter_val1); // $2: the slot_id
        if booking_id.is_none() {
            q = q.bind(filter_val2);
        }

        // execution of the query and handling errors
        q.fetch_one(executor)
            .await
            .map_err(db_err!("Failed to update payment status"))
    }

    pub async fn mark_fully_paid_by_manager_tx(
        executor: &mut sqlx::PgConnection,
        booking_id: i32,
    ) -> Result<Booking, AppError> {
        Self::update_payment_status_tx(executor, "Fully_Paid", Some(booking_id), None, None).await
    }

    /// Internal helper to fetch enriched bookings by a specific filter (user_id or game_id).
    async fn find_enriched_internal(
        executor: &mut sqlx::PgConnection,
        filter_clause: &str,
        id: i32,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError>
    {
        use crate::api::handlers::user_handlers::get_user_bookings::BookingDetail;
        use bigdecimal::BigDecimal;

        let query = format!(
            "SELECT b.id, b.slot_id, b.payment_status, b.booking_time, b.contact_name, b.contact_phone,
                    t.name as turf_name, g.name as game_name, s.final_price, s.start_time, s.end_time,
                    u.username, u.phone
             FROM bookings b
             JOIN slots s ON b.slot_id = s.id
             JOIN games g ON s.game_id = g.id
             JOIN turfs t ON g.turf_id = t.id
             LEFT JOIN users u ON b.user_id = u.id
             WHERE {} = $1
             ORDER BY b.booking_time DESC",
            filter_clause
        );

        let rows = sqlx::query(&query)
            .bind(id)
            .fetch_all(executor)
            .await
            .map_err(db_err!("Failed to fetch bookings"))?;

        let booking_details: Vec<BookingDetail> = rows
            .iter()
            .map(|row: &sqlx::postgres::PgRow| {
                let total_price: BigDecimal = row.get("final_price");
                let payment_status: String = row.get("payment_status");

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

                let user_name: Option<String> = row
                    .get::<Option<String>, _>("username")
                    .or_else(|| row.get::<Option<String>, _>("contact_name"));
                let user_phone: Option<String> = row
                    .get::<Option<String>, _>("phone")
                    .or_else(|| row.get::<Option<String>, _>("contact_phone"));

                BookingDetail {
                    id: row.get("id"),
                    slot_id: row.get("slot_id"),
                    booking_number: row.get("id"),
                    user_name,
                    user_phone,
                    turf_name: row.get("turf_name"),
                    game_name: row.get("game_name"),
                    total_price,
                    advance_paid,
                    due_amount,
                    payment_status,
                    booking_time: row.get("booking_time"),
                    start_time: row.get("start_time"),
                    end_time: row.get("end_time"),
                }
            })
            .collect();

        Ok(booking_details)
    }

    /// Fetches all bookings for a user with turf, game, and price details.
    pub async fn find_by_user_id_enriched(
        state: &crate::config::state::SharedState,
        user_id: i32,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError>
    {
        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;
        Self::find_enriched_internal(&mut *conn, "b.user_id", user_id).await
    }

    /// Deletes orphaned 'Unpaid' booking records whose slot is now 'Available'.
    /// This happens when a user initiates a booking but never pays within 15 minutes.
    /// The slot is first reset to 'Available' by the slot cleanup query, and then
    /// this query removes the dangling booking row that was left behind.
    /// Called by the background cleanup worker every 60 seconds, after slot reset.
    pub async fn delete_orphaned_unpaid_bookings(db: &sqlx::PgPool) -> Result<u64, AppError> {
        sqlx::query(
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
        sqlx::query("DELETE FROM bookings WHERE id = $1")
            .bind(booking_id)
            .execute(executor)
            .await
            .map_err(db_err!("Failed to delete booking"))?;
        Ok(())
    }

    /// Fetches all bookings for a specific game with turf, game, and price details.
    /// Used by managers and admins to see all bookings for a specific game.
    pub async fn find_by_game_id_enriched(
        state: &crate::config::state::SharedState,
        game_id: i32,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError>
    {
        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;
        Self::find_enriched_internal(&mut *conn, "g.id", game_id).await
    }
}
