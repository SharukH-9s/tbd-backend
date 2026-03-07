/* when a client sends a htpp request to any route, with the header containing the "Bearer eyJhbGciOiJIUzI1Ni...", we need to veify then fetch the claim infused in the jwt.
in the parameter of the handler functions, we will write (claim:Claims). this will be the extractor for the http request. from the http link, a Claim object will be built.
to do this, we need to  implement the FromRequestParts trait for the Claim struct. FromRequestParts is the interface Axum uses to turn an incoming HTTP request into a specific Rust type, in this case the type
is Claim.
so when a request hits a route,axum will see that the handler function has claim:Claims. So it will see if the FromRequestParts trait is implemented for the Claims. As we have implemented the trait,
FromRequestParts trait will make an internal struct including these informations taken from the http url.
{
    headers: Authorization: Bearer ... -> JWT
    method: GET, POST, DELETE -> we can block DELETE requests for certain users.
    uri: /api/v1/profile -> we can check if the user is allowed on this specific path.
    version: HTTP/1.1 -> General protocol info.
    extensions: (Internal data) -> Shared data from other pieces of middleware.
}

then in the from_request_parts implementation we will use these data, and make a Claim object and return it. thats how claim from claim:Claims will get the Claim objetct.
every FromRequestParts implementation follows a strict, mandatory structure required by the Axum framework.

impl<S> FromRequestParts<S> for YourStructName
where
    S: Send + Sync, // This constraint is fixed/mandatory
{
    // 1. Define what the server sends back if the "Bouncer" rejects the user
    type Rejection = Response;

    // 2. The standard function signature
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> { parts contains the headers, method, uri, and version.

        // --- YOUR LOGIC GOES HERE ---
        // You look at 'parts' and decide if you want to return Ok(YourStructName)
        // or Err(some_error_response)

        todo!() // Placeholder
    }
}
*/
use crate::domain::errors::{AppError, error_codes};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};

use super::types::Claims;
use crate::config::state::SharedState;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};

// the Extractor Implementation
#[axum::async_trait]
impl<S> FromRequestParts<S> for Claims
// S is the entire AppState
where
    S: Send + Sync, // S state must be safe to share between different CPU threads.
    SharedState: FromRef<S>, // We need the SharedState from the AppState...so we define a way to get the SharedState out of whole AppState which is S
{
    type Rejection = Response; // If extraction fails, return this response

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        //parts contains the headers, method, uri, and version.
        // S was the AppState, we get that in _state. then from _state, we pull the SharedState
        let state = crate::config::state::SharedState::from_ref(_state);

        // Get the Authorization header
        let auth_header = parts.headers.get("Authorization");
        let auth_header = match auth_header {
            Some(value) => match value.to_str() {
                Ok(value) => value,
                Err(_) => {
                    return Err(AppError {
                        code: error_codes::MISSING_TOKEN,
                        message: "Missing authorization header",
                        status: StatusCode::BAD_REQUEST,
                    }
                    .into_response());
                }
            },
            None => {
                return Err(AppError {
                    code: error_codes::MISSING_TOKEN,
                    message: "Missing authorization header",
                    status: StatusCode::BAD_REQUEST,
                }
                .into_response());
            }
        };

        // Check if it's a Bearer token and cut the prefix
        if !auth_header.starts_with("Bearer ") {
            return Err(AppError {
                code: error_codes::INVALID_TOKEN,
                message: "Invalid token",
                status: StatusCode::UNAUTHORIZED,
            }
            .into_response());
        }
        let token = &auth_header[7..]; // "Bearer " is 7 characters

        // Decode and Validate
        let secret = &state.jwt_secret;

        let token_data = match decode(
            &token,
            &DecodingKey::from_secret(secret.as_ref()),
            &Validation::default(),
        ) {
            Ok(token_data) => token_data,
            Err(e) => {
                if e.kind() == &jsonwebtoken::errors::ErrorKind::ExpiredSignature {
                    return Err(AppError {
                        code: error_codes::EXPIRED_TOKEN,
                        message: "Token has expired",
                        status: StatusCode::UNAUTHORIZED,
                    }
                    .into_response());
                } else {
                    return Err(AppError {
                        code: error_codes::JWT_DECODE_FAILED,
                        message: "Failed to decode the JWT!",
                        status: StatusCode::UNAUTHORIZED,
                    }
                    .into_response());
                }
            }
        };
        // Return the claims (the user's ID)
        Ok(token_data.claims)
        // http request >> Parts >> headers >> to_str() >> access_token >> decode() >> token data >> Claim
    }
}

/// The lifetime of an access token in seconds (15 minutes).
pub const ACCESS_TOKEN_LIFETIME_SECS: i64 = 900;

/// this will create a JWT access token for the given user id
pub fn make_access_token(user_id: i32, secret: &[u8]) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        iat: now.timestamp(),
        exp: (now + Duration::minutes(15)).timestamp(),
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret))
    .map_err(|_| AppError {
        code: error_codes::JWT_ENCODE_FAIL,
        message: "Couldn't encode the JWT!",
        status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
    })
}

/*
IntoResponse trait:
into_response() is the primary method of the IntoResponse trait. If a type implements this trait, it is telling Axum: "I know how to turn myself into a valid HTTP response."
HTTP response is a specific package consisting of:
1. A Status Code (e.g., 200 OK, 404 Not Found).
2. Headers (e.g., Content-Type: application/json).
3. A Body (the actual data).
When we call .into_response(), Axum takes our Rust types and "bakes" them into this three-part package.

we can call into_response() on a tuple which looks like (status, headers, body).
let status = StatusCode::CREATED;
let headers = [(header::AUTHORIZATION, "Bearer some-new-token")];
let body = Json(json!({ "message": "Resource created successfully" }));
then if we call (status, headers, body).into_response() we get axum::response::Response, a structured container that looks like this internally :
Status	201 Created
Header 1	Authorization: Bearer some-new-token
Header 2	Content-Type: application/json
Body	{"message":"..."}

An HTTP response is just a specifically formatted block of text. Here is what a typical response looks like:
HTTP/1.1 201 Created                     <-- 1. Protocol, StatusCode and reason phrase(Created)
Content-Type: application/json           <-- 2. Headers When we use Json(), Axum automatically adds Content-Type: application/json
Authorization: Bearer some-new-token     <-- 2. Headers (Custom)
Content-Length: 42                       <-- 2. Headers (Calculated by Axum)

{                                        <-- 3. The Body (The JSON data)
  "message": "Resource created"
}

*/
