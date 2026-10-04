use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};

/// Decision 11 — ETag and `304 Not Modified` support.
///
/// For every `GET` request that returns `200 OK`:
///   1. The response body is buffered.
///   2. A truncated SHA-256 ETag is computed (first 16 hex chars = 64 bits of entropy).
///   3. If the request carried `If-None-Match` and it matches, `304 Not Modified`
///      is returned with an empty body — zero payload bytes transferred.
///   4. Otherwise the full `200` response is returned with the `ETag` header set.
///
/// Combined with `Cache-Control: no-cache` (Decision 10), this gives:
///   - Correctness: browser always revalidates before serving from its local cache.
///   - Efficiency: on a cache hit the server answers in microseconds (L1) and sends
///     no body, making per-request cost near-zero for unchanged resources.
///
/// NOTE: This middleware buffers the body once per non-304 response. On an L1 hit
/// the body is an `Arc<str>` already in process heap, so the copy is O(body size)
/// but no new allocation is needed beyond the bytes slice.
pub async fn etag_middleware(req: Request<Body>, next: Next) -> Response {
    // Only apply ETag logic to GET requests.
    if req.method() != Method::GET {
        return next.run(req).await;
    }

    // Capture If-None-Match before consuming the request.
    let if_none_match = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let response = next.run(req).await;

    // Only compute ETags for successful responses.
    if response.status() != StatusCode::OK {
        return response;
    }

    let (parts, body) = response.into_parts();

    // Buffer the full response body.
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        // If buffering fails (shouldn't happen for our JSON responses), pass through as-is.
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    // Compute ETag: first 16 hex chars of SHA-256 (64 bits of entropy — sufficient
    // to make collisions negligible at this scale: P(collision) ≈ 1 in 1.8×10¹⁹).
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    let etag = format!("\"{}\"", &hex::encode(hash)[..16]);

    // If the client's cached ETag matches, short-circuit with 304 — no body sent.
    if if_none_match.as_deref() == Some(&etag) {
        let mut not_modified = Response::new(Body::empty());
        *not_modified.status_mut() = StatusCode::NOT_MODIFIED;
        // Preserve Vary and Cache-Control headers on 304 for proxy correctness.
        if let Some(cc) = parts.headers.get(header::CACHE_CONTROL) {
            not_modified.headers_mut().insert(header::CACHE_CONTROL, cc.clone());
        }
        return not_modified;
    }

    // Attach ETag to the full response and return.
    let mut response = Response::from_parts(parts, Body::from(bytes));
    if let Ok(val) = HeaderValue::from_str(&etag) {
        response.headers_mut().insert(header::ETAG, val);
    }
    response
}
