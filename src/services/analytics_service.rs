use chrono::{Datelike, Utc};
use crate::config::state::SharedState;
use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::analytics::{
    GameAnalytics, MonthlyAnalyticsFilter, MonthlyAnalyticsResponse, TurfAnalytics,
};
use crate::domain::models::users::Role;
use crate::repositories::analytics_repo::AnalyticsRepository;
use crate::repositories::turf_repo::TurfRepository;

pub struct AnalyticsService;

impl AnalyticsService {
    #[tracing::instrument(
        skip(state),
        fields(caller_id, ?caller_role, ?filter),
        name = "analytics.get_monthly"
    )]
    pub async fn get_monthly_analytics(
        state: &SharedState,
        caller_id: i32,
        caller_role: &Role,
        filter: MonthlyAnalyticsFilter,
    ) -> Result<MonthlyAnalyticsResponse, AppError> {
        let is_admin = matches!(caller_role, Role::Admin);
        let is_manager = matches!(caller_role, Role::Manager);

        if !is_admin && !is_manager {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "Only admins and managers can view monthly analytics",
            ));
        }

        // Determine default year/month using Asia/Dhaka time
        let now_dhaka = Utc::now().with_timezone(&chrono_tz::Asia::Dhaka);
        let default_year = now_dhaka.year();
        let default_month = now_dhaka.month() as i32;

        let (year, month) = match (filter.year, filter.month) {
            (None, None) => (default_year, Some(default_month)),
            (Some(y), None) => (y, None),
            (None, Some(m)) => (default_year, Some(m)),
            (Some(y), Some(m)) => (y, Some(m)),
        };

        // Input validation
        if !(2000..=2100).contains(&year) {
            return Err(AppError::bad_request(
                error_codes::BAD_REQUEST,
                "Year must be between 2000 and 2100",
            ));
        }

        if let Some(m) = month {
            if !(1..=12).contains(&m) {
                return Err(AppError::bad_request(
                    error_codes::BAD_REQUEST,
                    "Month must be between 1 and 12",
                ));
            }
        }

        // Scoping & permissions check on turf_id if provided
        if let Some(tid) = filter.turf_id {
            if is_manager {
                let turf = match TurfRepository::find_by_id(state, tid).await? {
                    Some(t) => t,
                    None => {
                        return Err(AppError::not_found(
                            error_codes::TURF_NOT_FOUND,
                            "Turf not found",
                        ))
                    }
                };

                if turf.manager_id != caller_id {
                    return Err(AppError::forbidden(
                        error_codes::FORBIDDEN,
                        "You can only view analytics for your own turfs",
                    ));
                }
            }
        }

        let manager_id = if is_admin { None } else { Some(caller_id) };

        let rows = AnalyticsRepository::fetch_monthly_analytics(
            state,
            year,
            month,
            manager_id,
            filter.turf_id,
        )
        .await?;

        // Group rows by turf
        let mut turfs: Vec<TurfAnalytics> = Vec::new();

        for row in rows {
            let game_analytics = GameAnalytics {
                game_id: row.game_id,
                game_name: row.game_name,
                total_bookings: row.total_bookings,
                fully_paid_bookings: row.fully_paid_bookings,
                income: row.income.clone(),
            };

            if let Some(existing_turf) = turfs.iter_mut().find(|t| t.turf_id == row.turf_id) {
                existing_turf.turf_total_income += &row.income;
                existing_turf.turf_total_bookings += row.total_bookings;
                existing_turf.turf_fully_paid_bookings += row.fully_paid_bookings;
                existing_turf.games.push(game_analytics);
            } else {
                turfs.push(TurfAnalytics {
                    turf_id: row.turf_id,
                    turf_name: row.turf_name,
                    games: vec![game_analytics],
                    turf_total_income: row.income,
                    turf_total_bookings: row.total_bookings,
                    turf_fully_paid_bookings: row.fully_paid_bookings,
                });
            }
        }

        Ok(MonthlyAnalyticsResponse {
            year,
            month,
            data: turfs,
        })
    }
}
