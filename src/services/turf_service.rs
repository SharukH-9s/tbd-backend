use crate::config::state::SharedState;
use crate::domain::models::turfs::Turf;
use crate::repositories::turf_repo::TurfRepository;

pub struct TurfService;

impl TurfService {
    pub async fn get_all_turfs(state: &SharedState) -> Result<Vec<Turf>, sqlx::Error> {
        TurfRepository::find_all(state).await
    }

    pub async fn create_turf(
        state: &SharedState,
        manager_id: i32,
        name: &str,
        location: &str,
        description: Option<&str>,
        price: f64,
    ) -> Result<Turf, sqlx::Error> {
        TurfRepository::insert(state, manager_id, name, location, description, price).await
    }
}
