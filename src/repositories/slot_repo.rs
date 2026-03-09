use crate::config::state::SharedState;
use crate::domain::models::slots::{Slot, SlotStatus};
use chrono::{DateTime, Utc};
use sqlx::query_as;

pub struct SlotRepository;

impl SlotRepository {
    pub async fn find_by_turf_and_date(
        state: &SharedState,
        turf_id: i32,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Slot>, sqlx::Error> {
        query_as::<_, Slot>(
            "SELECT * FROM slots WHERE turf_id = $1 AND start_time >= $2 AND start_time <= $3 ORDER BY start_time ASC"
        )
        .bind(turf_id)
        .bind(start)
        .bind(end)
        .fetch_all(&state.db)
        .await
    }

    pub async fn find_by_id(state: &SharedState, id: i32) -> Result<Option<Slot>, sqlx::Error> {
        query_as::<_, Slot>("SELECT * FROM slots WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
    }

    pub async fn update_status(
        state: &SharedState,
        id: i32,
        status: SlotStatus,
        pending_until: Option<DateTime<Utc>>,
    ) -> Result<Slot, sqlx::Error> {
        query_as::<_, Slot>(
            "UPDATE slots SET status = $1, pending_until = $2 WHERE id = $3 RETURNING *",
        )
        .bind(status)
        .bind(pending_until)
        .bind(id)
        .fetch_one(&state.db)
        .await
    }

    pub async fn bulk_insert(
        state: &SharedState,
        turf_id: i32,
        slots: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    ) -> Result<(), sqlx::Error> {
        for (start, end) in slots {
            sqlx::query("INSERT INTO slots (turf_id, start_time, end_time) VALUES ($1, $2, $3)")
                .bind(turf_id)
                .bind(start)
                .bind(end)
                .execute(&state.db)
                .await?;
        }
        Ok(())
    }

    pub async fn release_expired_slots(state: &SharedState) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("UPDATE slots SET status = 'Available', pending_until = NULL WHERE status = 'Pending' AND pending_until < NOW()")
            .execute(&state.db)
            .await?;
        Ok(result.rows_affected())
    }
}
