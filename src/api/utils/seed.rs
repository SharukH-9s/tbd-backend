use crate::config::state::SharedState;
use crate::domain::models::users::Role;
use crate::repositories::user_repo::UserRepository;
use std::env;

pub async fn ensure_admin_exists(state: &SharedState) {
    let admin_email = env::var("ADMIN_EMAIL").expect("ADMIN_EMAIL must be set in .env");
    let admin_password = env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set in .env");
    let admin_username = env::var("ADMIN_USERNAME").expect("ADMIN_USERNAME must be set in .env");

    // 1. Check if the admin already exists
    match UserRepository::find_by_email(state, &admin_email).await {
        Ok(Some(_)) => {
            println!("Admin user already exists: {}", admin_email);
            return;
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("Failed to check for admin user: {:?}", e);
            return;
        }
    }

    println!("Creating initial Admin user: {}...", admin_email);

    // 2. Hash the admin password
    let hashed_password = match bcrypt::hash(admin_password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to hash admin password: {}", e);
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
        Ok(_) => println!("Admin user created successfully: {}", admin_email),
        Err(e) => eprintln!("Failed to create admin user: {:?}", e),
    }
}
