# Implementation Plan: Observability (Logging, Distributed Tracing & Metrics)

## Goal Description
Implement a complete three-pillar observability suite (Structured Logging, Distributed OpenTelemetry Tracing, and Prometheus Metrics) to replace standard terminal outputs (`println!`) with production-grade monitoring tools.

---

## Why We Need It

Rather than plain text console logging, the `tracing` framework provides structured, asynchronous, levelled logging, contextual spans, and subscriber decoupling. The benefits of using `tracing` include:

### A. Structured Logging (Key-Value Pairs)
`tracing` allows you to bind variables as structured metadata:
```rust
tracing::info!(jti = %claims.jti, ttl = remaining_ttl, "Access token blacklisted in Redis");
```
When formatted (e.g., to JSON), this becomes a structured object that search/log analysis tools (like Datadog, Grafana Loki, or CloudWatch) can easily index, search, and filter on specific keys.

### B. Severity Levels
`tracing` provides standard logging severity levels to categorise logs based on their importance:
* `ERROR` – For critical failures (e.g., database unreachable in `/health`).
* `WARN` – For anomalies that aren't critical failures (e.g., rate limit hits).
* `INFO` – High-level system state changes (e.g., background worker cleanup completions).
* `DEBUG` – Detailed debugging information (e.g., checking which IP is keyed for rate limiting).
* `TRACE` – Extremely verbose execution traces.

In production, the subscriber can be configured to only emit `INFO` and above. In development, it can be set to `DEBUG` — all without changing any application code.

### C. Asynchronous and Non-Blocking
In high-throughput async servers (like our Axum backend), synchronous console writes can block the current thread and slow down the application under high load. `tracing` with non-blocking writers (`tracing-appender`) handles stdout writes on a separate background thread, keeping the main Tokio worker threads free.

### D. Contextual & Span Awareness (Request Tracing)
In asynchronous Rust (`tokio`), tasks are constantly suspended, resumed, and multiplexed, meaning logs from concurrent requests can interleave. `tracing` keeps track of which task belongs to which span in memory. Every time you log a message inside your code using standard macros like `info!()` or `error!()`, `tracing` automatically stamps the active span's context onto that log line before writing it out, making it easy to correlate logs from a single request.

### E. Decoupled Collection (Subscribers)
`tracing` decouples *what* is logged from *where* the output goes. The application code only defines the log payload, while a global **Subscriber** configured at startup determines where those logs actually go (e.g., pretty console print locally, JSON format for production, or an external collector over the network) without changing the log call sites.

### F. Prometheus Metrics for Real-time Monitoring
Exposing a `/metrics` endpoint allows infrastructure monitors (like Prometheus, Grafana, or Datadog) to continuously collect numeric health statistics from the running server. This includes:
- **Request latency histograms**: How long each route is taking to respond.
- **HTTP status counters**: How many 2xx/4xx/5xx responses are being returned.
- **SQLx connection pool gauges**: Active, idle, and max connections — critical for our Neon free tier's 20-connection ceiling.
- **Redis command latency**: Detect if rate limiter or JWT blacklist Redis calls are slowing down.

This gives us immediate visibility into resource exhaustion *before* it starts causing client-facing errors.

---

## How We Plan to Do It

### Step 1: Unified Logging Infrastructure (`tracing-subscriber`)
- Add `tracing-subscriber` (with `env-filter` and `json` features) and `tracing-appender` to `Cargo.toml`.
- In `main.rs`, initialize a global subscriber registry before any other startup code runs.
- **Environment-aware formatting**: Read an `APP_ENV` environment variable to select the output format:
  - `APP_ENV=development` → `fmt::Layer` with compact, coloured, human-readable output.
  - `APP_ENV=production` → `fmt::Layer::json()` for machine-readable JSON logs (ingested by Render log drains, Datadog, etc.).
- **Non-blocking writer**: Wrap the output layer with `tracing_appender::non_blocking(std::io::stdout())` so console writes are offloaded to a dedicated background thread. The returned `_guard` must be held for the lifetime of `main` to flush all pending logs on shutdown.
- **Filter level**: Use `EnvFilter::from_default_env()` so the minimum log level can be set at runtime via `RUST_LOG=info` (or `debug`, `warn`, etc.) without recompiling.

### Step 2: OpenTelemetry Distributed Tracing (`opentelemetry-otlp`)

**What is OpenTelemetry?**
OpenTelemetry (OTel) is a vendor-neutral, open standard for generating, collecting, and exporting telemetry data (traces, metrics, logs). Where Step 1 gives us *logs* (what happened), OTel Distributed Tracing gives us *spans* (a timed record of *work being done*) that can be stitched together across multiple services into a single **trace** — a complete timeline of one request's journey through your entire system.

A **span** has a start time, an end time, a name, and key-value attributes. When multiple spans reference each other as parent/child, they form a **trace tree**. This is how you answer: *"A user's booking request took 800ms — which part of the code was slow?"*

---

**New Crates Added to `Cargo.toml`**

| Crate | Role |
|---|---|
| `opentelemetry` | Core API crate. Defines the standard types: `Tracer`, `Span`, `TracerProvider`. |
| `opentelemetry_sdk` | The actual pipeline implementation: span processors, batch exporters, resource detection. The `rt-tokio` feature means the batch processor runs async on Tokio. |
| `opentelemetry-otlp` | The exporter component. Serializes spans into OTLP format and sends them over HTTP to a collector. The `reqwest-client` and `http-proto` features enable HTTP/Protobuf transport (simpler than gRPC — no cmake/protoc needed). |
| `tracing-opentelemetry` | The bridge. Translates every `tracing` span (created by `TraceLayer` or `#[tracing::instrument]`) into an OTel span and routes it to the pipeline. |
| `tower-http` (`trace` feature) | Adds the `TraceLayer` middleware that auto-creates an OTel span for every incoming HTTP request. |

---

**Implementation in `main.rs` — step by step**

**1. `dotenv()` runs first**
```rust
dotenv().ok();
```
The OTel exporter reads `OTEL_EXPORTER_OTLP_ENDPOINT` from the environment to know where to send spans. If `dotenv()` runs *after* the exporter is built, any value you set in `.env` is completely ignored. Order is critical here.

---

**2. W3C `TraceContext` Propagator**
```rust
opentelemetry::global::set_text_map_propagator(
    opentelemetry_sdk::propagation::TraceContextPropagator::new(),
);
```
This is what makes tracing *distributed*. The W3C `traceparent` header is a standard HTTP header that looks like:
```
traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
```
It encodes the parent trace ID and span ID. When the `TraceLayer` receives an HTTP request, it reads this header and links its newly created span as a *child* of the caller's span. Without registering this propagator globally, `TraceLayer` would ignore the header and every backend span would be a disconnected root — distributed tracing would be broken for cross-service calls.

---

**3. Resource — the "Who am I?" descriptor**
```rust
let resource = Resource::builder()
    .with_service_name("turf_bd_backend")
    .with_attribute(KeyValue::new("deployment.environment", app_env.clone()))
    .build();
```
A `Resource` is metadata that describes the entity generating the telemetry. Every single span exported will be permanently stamped with `service.name = "turf_bd_backend"` and `deployment.environment = "development"` (or `"production"`). In an observability dashboard, this is what lets you filter: *"show me only spans from the backend service in production"*.

---

**4. Exporter — sends spans over the network**
```rust
let exporter = SpanExporter::builder()
    .with_http()
    .build()
    .expect("Failed to build OTLP exporter");
```
The exporter takes finished spans and ships them via HTTP/Protobuf to an OTel Collector. The default endpoint is `http://localhost:4318/v1/traces`, but you override this at runtime with the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable — no recompile needed. The exporter is fault-tolerant by design: if the collector is unreachable, it logs a warning internally and drops the spans rather than crashing your application.

---

**5. `SdkTracerProvider` — the pipeline coordinator**
```rust
let tracer_provider = SdkTracerProvider::builder()
    .with_resource(resource)
    .with_batch_exporter(exporter)
    .build();

opentelemetry::global::set_tracer_provider(tracer_provider.clone());
```
The `SdkTracerProvider` ties the resource and exporter together. The key detail is `.with_batch_exporter()`: instead of making one HTTP request per span (which would be catastrophically slow under load), the SDK collects finished spans in an in-memory buffer and flushes them to the collector in periodic batches on a background Tokio task.

We call `.clone()` before passing it to `set_tracer_provider` so we retain our own handle — needed for the explicit shutdown at the end of `main`.

---

**6. OTel Layer — bridging `tracing` to OTel**
```rust
let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer);

tracing_subscriber::registry()
    .with(env_filter)
    .with(otel_layer)   // → OTel pipeline
    .with(json_layer)   // → stdout (production JSON)
    .with(text_layer)   // → stdout (development text)
    .init();
```
The `otel_layer` is the bridge crate at work. It subscribes to the `tracing` event stream. Every time `TraceLayer` (or `#[tracing::instrument]`) opens a `tracing` span, `otel_layer` simultaneously creates a matching OTel span and feeds it into the `SdkTracerProvider` pipeline. The same event reaches both the console formatter *and* the OTel exporter — two destinations, one source of truth.

---

**7. Axum `TraceLayer` — automatic HTTP request spans**
```rust
.layer(TraceLayer::new_for_http())
```
Added to the `final_router`. For every single HTTP request the server handles, `TraceLayer` automatically:
- Opens a new `tracing` span named after the request (e.g., `GET /slots/{id}/book`).
- Records the HTTP method, URL path, response status code, and total request duration as span attributes.
- Reads the incoming `traceparent` header (via the propagator registered in step 2) and sets the new span as a *child* of the caller's span. This is the mechanism that stitches your backend spans into a client's end-to-end trace.
- Closes the span when the response is sent, recording the duration automatically.

---

**8. Graceful Shutdown — flushing the buffer**
```rust
.with_graceful_shutdown(async {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Shutdown signal received, flushing telemetry");
})
.await
.unwrap();

if let Err(err) = tracer_provider.shutdown() {
    tracing::error!("Error shutting down tracer provider: {err:?}");
}
```
Because spans are batched in memory before being sent, a sudden `SIGTERM` or `Ctrl-C` would drop all buffered spans silently. `.with_graceful_shutdown()` waits for a shutdown signal, lets Axum drain in-flight HTTP requests, and then returns — allowing `tracer_provider.shutdown()` to execute, which force-flushes the remaining spans to the collector before the process exits. Zero telemetry data is lost on deployment restarts.


### Step 2.5: Code Instrumentation (Migrating Logs & Function Spans)

We will upgrade the application codebase to leverage the telemetry pipelines established in Steps 1 and 2 by standardizing event logs and instrumenting key operations.

#### 1. Standardizing Log Outputs
We will scan the codebase and replace all raw stdout/stderr prints (`println!` and `eprintln!`) with native `tracing` macro equivalents. This ensures that all runtime events are non-blocking, severity-leveled, and automatically capture the active span context:
- **`tracing::error!`** – Database query failures, cache misses that halt flow, and initialization errors.
- **`tracing::warn!`** – Handled anomalies, rate limit hits, invalid credentials, and payload validation failures.
- **`tracing::info!`** – Startup/shutdown events, database seeding, and scheduled cleanup worker executions.
- **`tracing::debug!`** – Low-level developer logs (e.g., token parsing checkpoints, client IP resolutions).

#### 2. Enhancing Context via `#[tracing::instrument]`
We will decorate critical handlers, service functions, and repository queries with `#[tracing::instrument]`. This automatically establishes a rich parent-child execution tree:
- **Automatic Sub-Span Lifecycles:** Automatically creates and wraps function executions as child spans parented under the Axum request span, measuring precise execution latency.
- **Structured Fields Mapping:** Enables explicit serialization of complex inputs using display formatting:
  ```rust
  #[tracing::instrument(skip(db), fields(user_id = %payload.user_id))]
  pub async fn create_booking(db: &PgPool, payload: CreateBookingPayload) -> ...
  ```
- **Diagnostic Safety (`skip`):** Prevents compile-time errors by skipping non-`Debug` types like `PgPool` or client configuration instances.

#### 3. Instrumentation Patterns & Strategy (Q&A)
To keep tracing highly maintainable and resource-efficient, we established clear guidelines on the style and placement of spans:

##### A. How many patterns of `#[tracing::instrument]` did we write?
We standardise on three distinct patterns based on the parameters involved:
1. **Type A: Skip-only (`skip(...)`)** — Used for functions containing non-`Debug` parameters (like connection pools) but having no interesting input fields to record:
   ```rust
   #[tracing::instrument(skip(database, old_token), name = "auth.refresh_access_token")]
   ```
2. **Type B: Skip + `fields(%var)` (Display)** — Used when recording key identifiers (like emails or usernames) that we want formatted using their `Display` implementation (`%`):
   ```rust
   #[tracing::instrument(skip(database, payload), fields(email = %payload.email), name = "auth.authenticate")]
   ```
3. **Type C: Skip + `fields(bare_param)` (Implicit Capture)** — Used for integer identifiers (like `slot_id` or `caller_id`) where matching the parameter name automatically captures and logs its value:
   ```rust
   #[tracing::instrument(skip(state), fields(slot_id, caller_id), name = "booking.initiate")]
   ```

##### B. Why only add `#[tracing::instrument]` to the Service Layer?
We deliberately avoided instrumenting handlers and database repositories to optimize performance and prevent trace clutter:
- **Handlers (HTTP Entry Points):** The root span for every request is already handled globally by Tower-HTTP's `TraceLayer` middleware. Adding `#[tracing::instrument]` to individual HTTP handler functions would create a redundant, nested child span with no new context.
- **Service Layer (Core Business Logic) ✅:** This is where transactional workflows, rule validation, and orchestration take place. Instrumenting this layer captures the high-level business action (e.g., `booking.initiate`) and associates it with the correct caller context.
- **Repository Layer (SQL Queries):** Database queries are thin wrappers around SQL calls. Instrumenting every repository method adds memory allocation overhead and results in trace-graph bloat. Since any repository error is mapped and bubble-up logged inside the parent Service span, individual query spans are generally unnecessary unless debugging specific database latency bottlenecks.

> [!TIP]
> **OpenTelemetry Trace Hierarchy Visualization**
>
> A request flowing through instrumented layers will construct a visual hierarchy inside the trace collector dashboard:
> ```
> ▼ [Axum HTTP Span] POST /slots/{id}/book (Total: 45ms)
>    │  ├── http.status_code: 200
>    │
>    └─▼ [Service Span] initiate_booking_handler() (Duration: 42ms)
>       │  ├── user_id: 102
>       │
>       ├───► [Log Event] "Checking slot availability in database"
>       │
>       └─▼ [Repository Span] fetch_slot_from_db() (Duration: 35ms)
>             └── db.statement: "SELECT * FROM slots WHERE id = $1"
> ```

---

#### ✅ Implementation Status — Completed

##### Service Layer — All Functions Instrumented

| Service | Function | Span Name | Status |
|---|---|---|---|
| `auth_service` | `authenticate` | `auth.authenticate` | ✅ |
| `auth_service` | `refresh_access_token` | `auth.refresh_access_token` | ✅ |
| `user_service` | `register_user` | `user.register` | ✅ |
| `user_service` | `update_profile` | `user.update_profile` | ✅ |
| `turf_service` | `get_all_turfs` | `turf.get_all` | ✅ |
| `turf_service` | `get_turf_by_id` | `turf.get_by_id` | ✅ |
| `turf_service` | `create_turf` | `turf.create` | ✅ |
| `turf_service` | `update_turf` | `turf.update` | ✅ |
| `turf_service` | `delete_turf` | `turf.delete` | ✅ |
| `game_service` | `create_game` | `game.create` | ✅ Added |
| `game_service` | `get_games_for_turf` | `game.get_for_turf` | ✅ Added |
| `game_service` | `get_game_by_id` | `game.get_by_id` | ✅ Added |
| `game_service` | `update_game` | `game.update` | ✅ Added |
| `game_service` | `delete_game` | `game.delete` | ✅ Added |
| `game_settings_service` | `create_settings` | `settings.create` | ✅ Added |
| `game_settings_service` | `get_settings_for_game` | `settings.get_for_game` | ✅ Added |
| `game_settings_service` | `update_settings` | `settings.update` | ✅ Added |
| `game_settings_service` | `delete_settings` | `settings.delete` | ✅ Added |
| `slot_service` | `create_slot` | `slot.create` | ✅ Added |
| `slot_service` | `get_slots_for_game_on_date` | `slot.get_for_game_on_date` | ✅ Added |
| `slot_service` | `create_custom_slot` | `slot.create_custom` | ✅ Added |
| `slot_service` | `delete_slot` | `slot.delete` | ✅ Added |
| `booking_service` | `initiate_booking` | `booking.initiate` | ✅ |
| `booking_service` | `update_payment` | `booking.update_payment` | ✅ |
| `booking_service` | `cancel_booking` | `booking.cancel` | ✅ |
| `booking_service` | `pay_advance` | `booking.pay_advance` | ✅ |
| `booking_service` | `pay_full` | `booking.pay_full` | ✅ |
| `booking_service` | `get_user_bookings` | `booking.get_user_bookings` | ✅ Added |
| `booking_service` | `get_game_bookings` | `booking.get_game_bookings` | ✅ Added |

##### Error Tracking — `AppError::into_response` records all errors into the active span

**Problem:** `IntoResponse for AppError` was completely silent — every `404`, `403`, `409`, `500` was converted to an HTTP response with zero tracing. Honeycomb showed all spans as green even on failures.

**Fix applied to [`errors.rs`](file:///e:/projects/anti/TBD/src/domain/errors.rs):**
- `span.record("error.code", ...)` and `span.record("error.message", ...)` written into the active span on every error — one choke-point covers all services and handlers automatically.
- **5xx errors** → `tracing::error!` → Honeycomb marks the trace as errored.
- **4xx errors** → `tracing::warn!` → visible in Honeycomb, but does not inflate the server error rate metric (these are expected client behaviour).

In Honeycomb you can now:
- Filter by `error.code = "GAME_NOT_FOUND"` to find all 404s on a specific domain object.
- Use `WHERE error.code EXISTS` to isolate only failed requests.
- Set up a trigger/alert on `http.status_code >= 500`.

##### Cleanup Worker — Structured Error Fields
`cleanup.rs` error logs were changed from unstructured `{:?}` string interpolation to structured `error = ?e` fields, making them queryable in Honeycomb rather than buried in the message text.


### Step 3: Prometheus Metrics Endpoint (`metrics-exporter-prometheus`)

#### Relationship with Step 2 (Traces vs. Metrics)
While **Step 2 (Traces)** gives you high-definition **deep dives** into individual request lifecycles (focusing on one transaction's timeline), **Step 3 (Metrics)** is **quantitative and aggregated**. It monitors system-wide numbers over time (counters, histograms, gauges) to answer: *"What is the total request volume per second right now?"* and *"Are we running out of database pool connections?"* Together, they complete the core pillars of our observability suite.

---

#### 1. Core Objectives
We expose a real-time, numeric telemetry endpoint at **`GET /metrics`**. Scraping infrastructure (like Prometheus, Grafana, or Datadog) regularly polls this endpoint to construct service health dashboards.

---

#### 2. Key Metrics Implemented

- **`http_requests_total` (Counter)**: 
  Increments by 1 every time an HTTP request completes. It is labeled by `method`, `path`, and HTTP `status` (e.g. `method="POST", path="/slots/{id}/book", status="201"`).
  
- **`http_request_duration_seconds` (Histogram)**:
  Measures request latency in seconds. Enables calculating percentiles (p50, p90, p99) to pinpoint performance degradation under load. Labeled by `method` and `path`.
  
- **`db_pool_connections_active` & `db_pool_connections_idle` (Gauges)**:
  Numeric gauges representing active vs. idle database connections. Crucial for monitoring Neon DB's free-tier connection ceiling.

---

#### 3. Component Details & Code Logic

##### A. Global Recorder Initialization
The global Prometheus recorder is registered inside `main.rs` before any other bootstrap code. This instantiates the in-memory metric registry that stores accumulated metrics. An optional `METRICS_BEARER_TOKEN` is loaded from the environment to control secure scraper access in production.

##### B. HTTP Middleware (`track_http_metrics`)
A custom Axum middleware captures HTTP metadata:
1. **Cardinality Protection (`MatchedPath`)**: Instead of raw URLs, the middleware reads Axum's parameterized route template (e.g., `/users/{id}`). This prevents cardinailty explosion in Prometheus (where thousands of unique user IDs would generate distinct metric keys and exhaust memory).
2. **System Noise Exclusions**: Common system probe routes (`/health`, `/ping`, `/metrics`) are skipped from measurement to keep metrics data clean.
3. **Execution Timer**: The middleware records request duration using monotonic timers, increments the request counter, and feeds the histogram.

##### C. Database Pool Monitor (`track_database_metrics`)
A dedicated background worker task runs every 5 seconds. It queries the SQLx `PgPool` configuration to extract `pool.size()` and `pool.num_idle()`, calculating:
$$\text{active connections} = \text{total size} - \text{idle connections}$$
It then publishes these gauges to the global recorder.

##### D. Guarded exposition route (`GET /metrics`)
Registers a route returning the Prometheus text exposition format.
- If `METRICS_BEARER_TOKEN` is present, it validates the incoming `Authorization: Bearer <token>` header, returning `401 Unauthorized` if mismatched.
- If absent, it bypasses security to allow local developer access.

> [!TIP]
> **Cardinality Control Tip**
> If you record metric values using raw request URLs (e.g. `/slots/1/book`, `/slots/2/book`), Prometheus creates separate series for each ID. Always group paths using route pattern mappings (`/slots/{id}/book`) to prevent database overload.

> [!CAUTION]
> **Secure `/metrics` in production.** This endpoint exposes internal application statistics. It must be restricted to the internal scraper network only (e.g., via a Cloudflare WAF rule blocking public access) or protected with a `Bearer` token middleware to prevent public enumeration of backend internals.


---

## Proposed Changes

### Cargo Dependencies

> [!WARNING]
> The OpenTelemetry Rust ecosystem (`opentelemetry`, `tracing-opentelemetry`, `opentelemetry-otlp`) releases breaking changes frequently. The versions below are illustrative. **Verify the latest compatible set on [crates.io](https://crates.io/crates/opentelemetry) before adding them.**

```toml
[dependencies]
# Tracing — structured, levelled, async logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# OpenTelemetry — distributed trace context propagation
opentelemetry = { version = "0.21", features = ["rt-tokio"] }
tracing-opentelemetry = "0.22"
opentelemetry-otlp = { version = "0.14", features = ["grpc-tonic"] }

# Metrics — Prometheus exposition format
metrics = "0.21"
metrics-exporter-prometheus = "0.12"
```

### Files to Change

| File | Change |
|---|---|
| **[Cargo.toml](file:///e:/projects/anti/TBD/Cargo.toml)** | Add the dependencies above. |
| **[main.rs](file:///e:/projects/anti/TBD/src/main.rs)** | Initialize subscriber registry, OTel pipeline, Prometheus recorder. Register `GET /metrics` route. |
| **[src/api/middleware/](file:///e:/projects/anti/TBD/src/api/middleware/)** | Add request tracing layer (`TraceLayer`) and HTTP metrics middleware to the router. |

---

## Appendix: Connecting Render → Honeycomb

> [!IMPORTANT]
> The code is already wired. All you need to do is set two environment variables on Render and Honeycomb will start receiving traces immediately.

### Step 1 — Get your Honeycomb API Key

1. Go to [honeycomb.io](https://ui.honeycomb.io) and log in.
2. Click your team name (top-left) → **Team Settings** → **API Keys**.
3. Click **Create API Key**.
   - Give it a name (e.g. `render-production`).
   - Enable the **Send Events** permission.
4. Copy the key — you only see it once.

### Step 2 — Set the environment variables on Render

Go to your Render service → **Environment** tab and add:

| Variable | Value |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `https://api.honeycomb.io` |
| `OTEL_EXPORTER_OTLP_HEADERS` | `x-honeycomb-team=YOUR_API_KEY` |

> [!IMPORTANT]
> Use `https://api.honeycomb.io` (without `/v1/traces`). The OpenTelemetry SDK automatically appends `/v1/traces` to `OTEL_EXPORTER_OTLP_ENDPOINT`. If you include `/v1/traces` in `OTEL_EXPORTER_OTLP_ENDPOINT`, the SDK will send requests to `https://api.honeycomb.io/v1/traces/v1/traces` causing network/404 export errors.
> (Alternatively, if you want to use the full path, use the variable name `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT = https://api.honeycomb.io/v1/traces`).

### Step 3 — Verify in Honeycomb

1. Trigger any request against your Render API (e.g. `GET /health`).
2. In Honeycomb, open **Home** → your dataset will appear automatically (named `turf_bd_backend` from the `service.name` resource attribute).
3. Click into it → **New Query** → you should see spans within a few seconds.

### Filtering out noise — hide health check spans

The `/health` endpoint fires every 30 seconds from Render's uptime monitor and pollutes your trace view. To hide it:

In Honeycomb **Query** builder, add a **WHERE** clause:
```
http.target != /health
```
Save it as a **Board** so it applies to all your default views.

### What you'll see per request

```
▼ POST /turfs/1/games  (root — created by TraceLayer)
   └─▼ game.create     (child — created by #[tracing::instrument])
        ├── turf_id: 1
        ├── caller_id: 42
        └── game_name: "Football"
```

Any request that hits an `AppError` will also show:
```
error.code:    GAME_NOT_FOUND
error.message: Game not found
http.status_code: 404
```
