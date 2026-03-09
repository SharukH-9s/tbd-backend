use crate::config::state::SharedState;
use crate::domain::models::bookings::{Booking, PaymentStatus};
use sqlx::query_as;

pub struct BookingRepository;

impl BookingRepository {
    pub async fn insert(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, sqlx::Error> {
        query_as::<_, Booking>(
            "INSERT INTO bookings (slot_id, user_id) VALUES ($1, $2) RETURNING *",
        )
        .bind(slot_id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
    }

    pub async fn update_payment_status(
        state: &SharedState,
        booking_id: i32,
        status: PaymentStatus,
    ) -> Result<Booking, sqlx::Error> {
        query_as::<_, Booking>("UPDATE bookings SET payment_status = $1 WHERE id = $2 RETURNING *")
            .bind(status)
            .bind(booking_id)
            .fetch_one(&state.db)
            .await
    }

    pub async fn find_by_user(
        state: &SharedState,
        user_id: i32,
    ) -> Result<Vec<Booking>, sqlx::Error> {
        query_as::<_, Booking>(
            "SELECT * FROM bookings WHERE user_id = $1 ORDER BY booking_time DESC",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await
    }
}
