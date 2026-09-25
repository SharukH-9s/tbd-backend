use crate::config::state::SharedState;
use crate::db_conn_err;
use crate::db_err;
use crate::domain::errors::AppError;
use bigdecimal::BigDecimal;

#[derive(sqlx::FromRow)]
pub struct AnalyticsRawRow {
    pub turf_id: i32,
    pub turf_name: String,
    pub game_id: i32,
    pub game_name: String,
    pub total_bookings: i64,
    pub fully_paid_bookings: i64,
    pub income: BigDecimal,
}

pub struct AnalyticsRepository;

impl AnalyticsRepository {
    pub async fn fetch_monthly_analytics(
        state: &SharedState,
        year: i32,
        month: Option<i32>,
        manager_id: Option<i32>,
        turf_id: Option<i32>,
    ) -> Result<Vec<AnalyticsRawRow>, AppError> {
        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;

        let rows = sqlx::query_as::<sqlx::Postgres, AnalyticsRawRow>(
            r#"SELECT
                t.id AS turf_id,
                t.name AS turf_name,
                g.id AS game_id,
                g.name AS game_name,
                COUNT(b.id) AS total_bookings,
                COUNT(b.id) FILTER (WHERE b.payment_status = 'Fully_Paid') AS fully_paid_bookings,
                COALESCE(SUM(b.total_price) FILTER (WHERE b.payment_status = 'Fully_Paid'), 0::NUMERIC) AS income
             FROM bookings b
             JOIN slots s ON b.slot_id = s.id
             JOIN games g ON s.game_id = g.id
             JOIN turfs t ON g.turf_id = t.id
             WHERE EXTRACT(YEAR FROM (
                 CASE
                     WHEN g.game_end_time < g.game_start_time
                          AND (s.start_time AT TIME ZONE 'Asia/Dhaka')::time < g.game_end_time
                     THEN (s.start_time AT TIME ZONE 'Asia/Dhaka') - INTERVAL '1 day'
                     ELSE (s.start_time AT TIME ZONE 'Asia/Dhaka')
                 END
             ))::INT = $1
               AND ($2::INT IS NULL OR EXTRACT(MONTH FROM (
                 CASE
                     WHEN g.game_end_time < g.game_start_time
                          AND (s.start_time AT TIME ZONE 'Asia/Dhaka')::time < g.game_end_time
                     THEN (s.start_time AT TIME ZONE 'Asia/Dhaka') - INTERVAL '1 day'
                     ELSE (s.start_time AT TIME ZONE 'Asia/Dhaka')
                 END
             ))::INT = $2)
               AND ($3::INT IS NULL OR t.manager_id = $3)
               AND ($4::INT IS NULL OR t.id = $4)
             GROUP BY t.id, t.name, g.id, g.name
             ORDER BY t.name ASC, g.name ASC"#,
        )
        .bind(year)
        .bind(month)
        .bind(manager_id)
        .bind(turf_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(db_err!("Failed to fetch monthly analytics"))?;

        Ok(rows)
    }
}
