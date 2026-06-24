use crate::{db_conn_err, db_err};
use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::domain::models::games::Game;
use chrono::NaiveTime;
use sqlx::query_as;

pub struct GameRepository;

impl GameRepository {
    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        turf_id: i32,
        name: &str, // deref coercion
        game_start_time: NaiveTime,
        game_end_time: NaiveTime,
    ) -> Result<Game, AppError> {
        query_as::<_, Game>(
            "INSERT INTO games (turf_id, name, game_start_time, game_end_time)
             VALUES ($1, $2, $3, $4)
             RETURNING *",
        )
        .bind(turf_id)
        .bind(name)
        .bind(game_start_time)
        .bind(game_end_time)
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to insert game into database")) // Result<Game, AppError>
    }

    pub async fn find_all_games_by_turf_id(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<Game>, AppError> {
        query_as::<_, Game>("SELECT * FROM games WHERE turf_id = $1 ORDER BY id ASC")
            .bind(turf_id)
            .fetch_all(&state.db)
            .await
            .map_err(db_err!("Failed to fetch games from database")) // Result<Vec<Game>, AppError>
    }

    pub async fn find_by_id(
        state: &SharedState,
        id: i32,
    ) -> Result<Option<Game>, AppError> {
        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;
        Self::find_by_id_tx(&mut *conn, id).await
    }

    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<Game>, AppError> {
        query_as::<_, Game>("SELECT * FROM games WHERE id = $1")
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(db_err!("Failed to fetch game from database (tx)")) // Result<Option<Game>, AppError>
    }

    pub async fn update_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
        name: &str, // deref coercion
        game_start_time: NaiveTime,
        game_end_time: NaiveTime,
    ) -> Result<Game, AppError> {
        query_as::<_, Game>(
            "UPDATE games SET name = $1, game_start_time = $2, game_end_time = $3
             WHERE id = $4
             RETURNING *",
        )
        .bind(name)
        .bind(game_start_time)
        .bind(game_end_time)
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to update game in database")) // Result<Game, AppError>
    }

    pub async fn delete_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("DELETE FROM games WHERE id = $1")
            .bind(id)
            .execute(executor)
            .await
            .map_err(db_err!("Failed to delete game from database")) // Result<sqlx::postgres::PgQueryResult, AppError>
    }
}
