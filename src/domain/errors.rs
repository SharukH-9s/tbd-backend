use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::Span;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody, // error is the root key for the error response
}

#[derive(Serialize)]
pub struct ErrorBody {
    //
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub struct AppError {
    // struct to hold the error code, message, and status code
    pub code: &'static str,
    pub message: &'static str,
    pub status: StatusCode,
}

pub mod error_codes {
    pub const INVALID_INPUT: &str = "INVALID_INPUT";
    #[allow(dead_code)]
    pub const INVALID_USERNAME: &str = "INVALID_USERNAME";
    pub const USERNAME_EXISTS: &str = "USERNAME_EXISTS";
    pub const EMAIL_EXISTS: &str = "EMAIL_EXISTS";
    pub const HASH_FAILED: &str = "HASH_FAILED";
    pub const UN_HASH_FAILED: &str = "UNHASH_FAILED";
    pub const INVALID_CREDENTIALS: &str = "INVALID_CREDENTIALS"; // generic — covers both unknown email and wrong password
    pub const JWT_ENCODE_FAIL: &str = "JWT_ENCODE_FAILED";
    pub const NO_REFRESH_TOKEN: &str = "NO_REFRESH_TOKEN";
    pub const INVALID_REFRESH_TOKEN: &str = "INVALID_REFRESH_TOKEN";

    pub const MISSING_TOKEN: &str = "MISSING_TOKEN";
    pub const INVALID_TOKEN: &str = "INVALID_TOKEN";
    pub const EXPIRED_TOKEN: &str = "EXPIRED_TOKEN";
    pub const REVOKED_TOKEN: &str = "REVOKED_TOKEN"; // JWT has been blacklisted via Redis (user logged out)
    pub const JWT_DECODE_FAILED: &str = "JWT_DECODE_FAILED";
    pub const RATE_LIMIT_EXCEEDED: &str = "RATE_LIMIT_EXCEEDED"; // Too many requests in the rate-limit window
    pub const DATABASE_ERROR: &str = "DATABASE_ERROR";

    // Turf & Manager specific
    pub const TRANSACTION_START_FAILED: &str = "TRANSACTION_START_FAILED";
    pub const TRANSACTION_COMMIT_FAILED: &str = "TRANSACTION_COMMIT_FAILED";
    pub const USER_DETAILS_REQUIRED: &str = "USER_DETAILS_REQUIRED";
    pub const USER_NOT_FOUND: &str = "USER_NOT_FOUND";
    pub const TURF_NOT_FOUND: &str = "TURF_NOT_FOUND";
    pub const FORBIDDEN: &str = "FORBIDDEN";
    pub const MANAGER_ALREADY_ASSIGNED: &str = "MANAGER_ALREADY_ASSIGNED";
    pub const GAME_NOT_FOUND: &str = "GAME_NOT_FOUND";
    pub const SETTINGS_NOT_FOUND: &str = "SETTINGS_NOT_FOUND";

    // Slot specific
    pub const SLOT_NOT_FOUND: &str = "SLOT_NOT_FOUND";
    pub const SLOT_EXCEEDS_BOUNDARY: &str = "SLOT_EXCEEDS_BOUNDARY";
    pub const SLOT_NOT_DELETABLE: &str = "SLOT_NOT_DELETABLE";
    pub const SLOT_INVALID_STATUS: &str = "SLOT_INVALID_STATUS";
    pub const SLOT_OVERLAP: &str = "SLOT_OVERLAP";
    pub const BOTH_CONTACT_NAME_AND_PHONE_REQUIRED: &str = "BOTH_CONTACT_NAME_AND_PHONE_REQUIRED";
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    pub const GAME_WINDOW_CONFLICT: &str = "GAME_WINDOW_CONFLICT";
    pub const GAME_HAS_ACTIVE_BOOKINGS: &str = "GAME_HAS_ACTIVE_BOOKINGS"; // narrowing the game window would orphan active slots

    // Booking specific
    pub const SLOT_UNAVAILABLE: &str = "SLOT_UNAVAILABLE"; // slot is Pending or Booked — cannot be booked
    pub const TOO_MANY_PENDING_BOOKINGS: &str = "TOO_MANY_PENDING_BOOKINGS"; // user already holds 2 pending slots
    pub const BOOKING_NOT_FOUND: &str = "BOOKING_NOT_FOUND";
    pub const BOOKING_NOT_CONFIRMED: &str = "BOOKING_NOT_CONFIRMED"; // slot is not Booked — cannot settle
    pub const ALREADY_SETTLED: &str = "ALREADY_SETTLED"; // booking is not Partially_Paid — cannot settle
    pub const PAYMENT_NOT_ALLOWED: &str = "PAYMENT_NOT_ALLOWED"; // slot is not Pending or booking is not Unpaid
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";

    // Photo specific
    pub const PHOTO_NOT_FOUND: &str = "PHOTO_NOT_FOUND";
    pub const PHOTO_LIMIT_EXCEEDED: &str = "PHOTO_LIMIT_EXCEEDED";
    pub const PHOTO_TOO_LARGE: &str = "PHOTO_TOO_LARGE";
    pub const INVALID_PHOTO_FORMAT: &str = "INVALID_PHOTO_FORMAT";
    pub const NO_PHOTOS_PROVIDED: &str = "NO_PHOTOS_PROVIDED";
}

// In any error case, we will create an AppError object. As AppError has Intoresponse implemnted on it, that object will be transformed into a response by axum
// if a function returns impl IntoResponse, then it can return an AppError as it implements IntoResponse trait.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status;
        let is_server_error = status.is_server_error();

        // Record the error into the current span so it appears in Honeycomb
        // regardless of which handler or service produced it.
        let span = Span::current();
        span.record("error.code", self.code);
        span.record("error.message", self.message);
        span.record("http.status_code", status.as_u16());

        if is_server_error {
            // Mark the span as failed — Honeycomb will highlight it as an error trace
            tracing::error!(
                error.code = self.code,
                error.message = self.message,
                http.status_code = status.as_u16(),
                "Request failed with server error"
            );
        } else {
            // 4xx are expected client errors — log at WARN so they are visible
            // but don't pollute the error rate metric
            tracing::warn!(
                error.code = self.code,
                error.message = self.message,
                http.status_code = status.as_u16(),
                "Request rejected with client error"
            );
        }

        let body = ErrorResponse {
            error: ErrorBody {
                code: self.code.to_string(),
                message: self.message.to_string(),
            },
        };

        (status, Json(body)).into_response()
    }
}

impl AppError {
    pub fn not_found(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::NOT_FOUND,
        }
    }
    pub fn forbidden(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::FORBIDDEN,
        }
    }
    pub fn conflict(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::CONFLICT,
        }
    }
    pub fn internal(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    pub fn bad_request(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::BAD_REQUEST,
        }
    }
    pub fn unauthorized(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::UNAUTHORIZED,
        }
    }
    pub fn service_unavailable(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/*
we could easily skip the ErrorResponse struct and just return the ErrorBody directly.
Most professional APIs choose a standard "root" key for different types of responses so the frontend knows exactly where to look.
Error root: { "error": { ... } }

If we use ErrorResponse, our JSON looks like this:

JSON
{
  "error": {
    "code": "INVALID_TOKEN",
    "message": "Bearer prefix missing"
  }
}
If we remove it and just use ErrorBody, it looks like this:

JSON
{
  "code": "INVALID_TOKEN",
  "message": "Bearer prefix missing"
} */

/// Produces a closure suitable for `.map_err()` on any sqlx query.
/// Prints the underlying error and maps it to a DATABASE_ERROR AppError.
///
/// Usage:
/// ```
/// .map_err(db_err!("Failed to fetch user from database"))
/// ```
#[macro_export]
macro_rules! db_err {
    ($msg:expr) => {
        |e| {
            tracing::error!(error = %e, "Database error");
            $crate::domain::errors::AppError {
                code: $crate::domain::errors::error_codes::DATABASE_ERROR,
                message: $msg,
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
    };
}

/// Produces a closure for `.map_err()` specifically on `pool.acquire()` calls.
/// Semantically identical to `db_err!` but signals a connection-pool failure at the call site.
///
/// Usage:
/// ```
/// let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;
/// ```
#[macro_export]
macro_rules! db_conn_err {
    () => {
        |e| {
            tracing::error!(error = %e, "Failed to acquire database connection from pool");
            $crate::domain::errors::AppError {
                code: $crate::domain::errors::error_codes::DATABASE_ERROR,
                message: "Internal server error",
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
    };
}
