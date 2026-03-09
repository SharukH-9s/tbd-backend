use crate::config::state::SharedState;
use crate::domain::models::turfs::Turf;
use sqlx::query_as;

pub struct TurfRepository;

impl TurfRepository {
    pub async fn find_all(state: &SharedState) -> Result<Vec<Turf>, sqlx::Error> {
        query_as::<_, Turf>("SELECT * FROM turfs ORDER BY name ASC")
            .fetch_all(&state.db)
            .await
    }

    pub async fn find_by_id(state: &SharedState, id: i32) -> Result<Option<Turf>, sqlx::Error> {
        query_as::<_, Turf>("SELECT * FROM turfs WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
    }

    pub async fn insert(
        state: &SharedState,
        manager_id: i32,
        name: &str,
        location: &str,
        description: Option<&str>,
        price: f64,
    ) -> Result<Turf, sqlx::Error> {
        query_as::<_, Turf>(
            "INSERT INTO turfs (manager_id, name, location, description, price_per_hour)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *",
        )
        .bind(manager_id)
        .bind(name)
        .bind(location)
        .bind(description)
        .bind(price)
        .fetch_one(&state.db)
        .await
    }

    pub async fn update(
        state: &SharedState,
        id: i32,
        name: &str,
        location: &str,
        description: Option<&str>,
        price: f64,
    ) -> Result<Turf, sqlx::Error> {
        query_as::<_, Turf>(
            "UPDATE turfs SET name = $1, location = $2, description = $3, price_per_hour = $4, updated_at = NOW()
             WHERE id = $5
             RETURNING *"
        )
        .bind(name)
        .bind(location)
        .bind(description)
        .bind(price)
        .bind(id)
        .fetch_one(&state.db)
        .await
    }

    pub async fn delete(state: &SharedState, id: i32) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM turfs WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await?;
        Ok(())
    }
}
