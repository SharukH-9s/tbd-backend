use crate::api::extractors::types::Claims;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::Role;
use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};

/// Middleware to ensure the user is authenticated.
/// Validates the JWT (via the Claims extractor) and injects the Claims into the request extensions.
pub async fn require_auth(
    claims: Claims,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

/// Middleware to ensure the user is an Admin.
/// Validates the JWT, checks for the Admin role, and injects the Claims into the request extensions.
pub async fn require_admin(
    claims: Claims,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !matches!(claims.role, Role::Admin) {
        return Err(AppError::forbidden(
            error_codes::FORBIDDEN,
            "Only administrators can access this resource",
        ));
    }
    
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

/// Middleware to ensure the user is a Manager or Admin.
/// Validates the JWT, checks for Manager/Admin role, and injects the Claims into the request extensions.
pub async fn require_manager_or_admin(
    claims: Claims,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !matches!(claims.role, Role::Admin) && !matches!(claims.role, Role::Manager) {
        return Err(AppError::forbidden(
            error_codes::FORBIDDEN,
            "Only managers and administrators can access this resource",
        ));
    }

    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}
