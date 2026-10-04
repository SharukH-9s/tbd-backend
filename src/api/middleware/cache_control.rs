use axum::{
    body::Body,
    http::{header, HeaderValue, Request},
    middleware::Next,
    response::Response,
};

/// Decision 10 — HTTP Cache-Control: Force Browser Revalidation on Every Request.
///
/// Attaches `Cache-Control: no-cache, max-age=0, must-revalidate` to every response.
///
/// `no-cache` does **not** mean "do not cache". It instructs the browser to always
/// revalidate with the server before serving from its local cache. Combined with the
/// ETag middleware (Decision 11), the browser sends a cheap conditional `If-None-Match`
/// request and the server can answer with `304 Not Modified` (no body transfer) if
/// nothing has changed.
///
/// This prevents browsers from applying heuristic caching to GET responses and serving
/// stale data without ever contacting the backend — which would make the server-side
/// L1/L2 invalidation logic completely irrelevant for those clients.
pub async fn cache_control_middleware(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        // SAFETY: this is a valid static header value — unwrap is intentional.
        HeaderValue::from_static("no-cache, max-age=0, must-revalidate"),
    );
    response
}
