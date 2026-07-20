use crate::config::state::SharedState;
use crate::domain::models::users::Role;
use crate::repositories::user_repo::UserRepository;
use std::env;

#[tracing::instrument(skip(state), name = "seed.ensure_admin_exists")]
pub async fn ensure_admin_exists(state: &SharedState) {
    let admin_email = env::var("ADMIN_EMAIL").expect("ADMIN_EMAIL must be set in .env");
    let admin_password = env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set in .env");
    let admin_username = env::var("ADMIN_USERNAME").expect("ADMIN_USERNAME must be set in .env");

    // 1. Check if the admin already exists
    match UserRepository::find_by_email(state, &admin_email).await {
        Ok(Some(_)) => {
            tracing::info!(email = %admin_email, "Admin user already exists — skipping seed");
            return;
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = ?e, "Failed to query for admin user during seed");
            return;
        }
    }

    tracing::info!(email = %admin_email, "Admin user not found — creating initial Admin");

    // 2. Hash the admin password
    let hashed_password = match bcrypt::hash(admin_password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "Failed to hash admin password during seed");
            return;
        }
    };

    // 3. Insert the admin into the database
    // Signature: (username, email, hashed_pass, phone, role)
    let user_data = (
        admin_username,
        admin_email.clone(),
        hashed_password,
        "0000000000".to_string(), // Default phone for root admin
        Role::Admin,
    );

    match UserRepository::insert_user_if_unique(state, user_data).await {
        Ok(_) => tracing::info!(email = %admin_email, "Admin user created successfully"),
        Err(e) => tracing::error!(error = ?e, email = %admin_email, "Failed to create admin user during seed"),
    }
}
