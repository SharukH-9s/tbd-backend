//! Outbox Relay — reads `queued` rows from the `outbox` table and
//! publishes them to CloudAMQP (RabbitMQ). Runs as a background
//! `tokio::spawn` task started from `main.rs`.

use lapin::{
    options::{
        BasicPublishOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const EXCHANGE_NAME: &str = "tbd.events";
const MAX_RETRIES: i32 = 5;

// ── Entry Point ───────────────────────────────────────────────────────────────

/// Start the relay loop. Called via `tokio::spawn(run_relay(db, amqp_url))` in main.rs.
pub async fn run_relay(db: PgPool, amqp_url: String) {
    // Read poll interval once at startup so every reconnect reuses the same value.
    // Override via RELAY_POLL_INTERVAL_SECS in .env (default: 5 s).
    let poll_secs: u64 = std::env::var("RELAY_POLL_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);

    tracing::info!(
        poll_interval_secs = poll_secs,
        "Relay task started — connecting to CloudAMQP..."
    );

    loop {
        match connect_and_run(&db, &amqp_url, poll_secs).await {
            // connect to AMQP and run the poll loop
            Ok(_) => tracing::warn!("Relay: AMQP connection closed — reconnecting in 5s..."),
            Err(e) => {
                tracing::error!(error = %e, "Relay: connection error — reconnecting in 5s...")
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

// ── Connection & Topology Setup ───────────────────────────────────────────────

async fn connect_and_run(db: &PgPool, amqp_url: &str, poll_secs: u64) -> Result<(), lapin::Error> {
    let conn = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
    tracing::info!("Relay: connected to CloudAMQP");

    let channel = conn.create_channel().await?; // create a channel on the connection.
    setup_topology(&channel).await?;

    // Recover any rows left in 'publishing' state from a previous crash/restart
    if let Err(e) = recover_stale_publishing_rows(db).await {
        tracing::error!(error = %e, "Relay: failed to recover stale publishing rows");
    }

    tracing::info!(
        poll_interval_secs = poll_secs,
        "Relay: topology ready — polling outbox"
    );
    poll_loop(db, &channel, poll_secs).await;
    Ok(())
}

/// Reset orphaned 'publishing' rows that may have been interrupted by a crash.
async fn recover_stale_publishing_rows(db: &PgPool) -> anyhow::Result<()> {
    let rows = sqlx::query!(
        r#"
        UPDATE outbox
        SET    status = 'queued',
               updated_at = NOW()
        WHERE  status = 'publishing'
        AND    updated_at < NOW() - INTERVAL '5 minutes'
        "#
    )
    .execute(db)
    .await?;

    if rows.rows_affected() > 0 {
        tracing::warn!(
            count = rows.rows_affected(),
            "Relay: recovered stale 'publishing' rows back to 'queued'"
        );
    }
    Ok(())
}

/// Declare the exchange and queues idempotently (durable = true).
/// RabbitMQ creates them on first run; subsequent runs are no-ops.
async fn setup_topology(ch: &Channel) -> Result<(), lapin::Error> {
    // Enable publisher confirms
    ch.confirm_select(ConfirmSelectOptions::default()).await?;

    // 1. Declare the DLX (Dead Letter Exchange)
    ch.exchange_declare(
        "tbd.dlx",
        ExchangeKind::Fanout,
        ExchangeDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await?;

    // 2. Declare the DLQ (Dead Letter Queue) and bind it to DLX
    ch.queue_declare(
        "tbd.dlq",
        QueueDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await?;
    ch.queue_bind(
        "tbd.dlq",
        "tbd.dlx",
        "",
        QueueBindOptions::default(),
        FieldTable::default(),
    )
    .await?;

    // 3. Declare the main exchange as a TOPIC exchange for selective routing
    ch.exchange_declare(
        EXCHANGE_NAME,
        ExchangeKind::Topic,
        ExchangeDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await?;

    // 4. Declare booking_jobs queue with DLX arguments
    let mut booking_args = FieldTable::default();
    booking_args.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString("tbd.dlx".into()),
    );

    ch.queue_declare(
        "booking_jobs", // name of the queue
        QueueDeclareOptions {
            durable: true,
            ..Default::default()
        },
        booking_args,
    )
    .await?;

    // Bind booking_jobs to receive all booking.* events
    ch.queue_bind(
        "booking_jobs",
        EXCHANGE_NAME,
        "booking.#",
        QueueBindOptions::default(),
        FieldTable::default(),
    )
    .await?;

    tracing::info!("Relay: topic exchanges and queues declared (including DLX)");
    Ok(())
}

// ── Poll Loop (Drain Loop) ───────────────────────────────────────────────────

async fn poll_loop(db: &PgPool, ch: &Channel, poll_secs: u64) {
    let mut ticker = interval(Duration::from_secs(poll_secs));
    loop {
        ticker.tick().await;

        if !ch.status().connected() {
            tracing::warn!("Relay: AMQP channel disconnected — exiting poll loop to reconnect");
            break;
        }

        // Drain loop: continuously process all ready rows until outbox is empty
        while ch.status().connected() {
            match process_one(db, ch).await {
                Ok(true) => {
                    // Processed a row, immediately check for the next
                    continue;
                }
                Ok(false) => {
                    // Outbox is empty for now, sleep until next tick
                    break;
                }
                Err(e) => {
                    tracing::error!(error = %e, "Relay: error processing outbox row");
                    break;
                }
            }
        }
    }
}

// ── Claim & Publish One Row ───────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: Uuid,
    event_type: String,
    payload: serde_json::Value,
    retry_count: i32,
}

/// Convert CamelCase/PascalCase event types to dot-notation routing keys.
/// e.g. "BookingCreated" -> "booking.created"
fn to_routing_key(event_type: &str) -> String {
    let mut key = String::new();
    for (i, ch) in event_type.chars().enumerate() {
        if ch.is_uppercase() && i > 0 && !key.ends_with('.') {
            key.push('.');
        }
        key.push(ch.to_ascii_lowercase());
    }
    key
}

async fn process_one(db: &PgPool, ch: &Channel) -> anyhow::Result<bool> {
    // Atomically claim one ready row. FOR UPDATE SKIP LOCKED prevents two relay
    // instances from ever claiming the same row simultaneously.
    let row = sqlx::query_as::<_, OutboxRow>(
        r#"
        UPDATE outbox
        SET    status = 'publishing', updated_at = NOW()
        WHERE  id = (
            SELECT id FROM outbox
            WHERE  status = 'queued'
            AND    (process_after IS NULL OR process_after <= NOW())
            ORDER  BY created_at ASC
            LIMIT  1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, event_type, payload, retry_count
        "#,
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Ok(false); // Nothing to process this tick
    };

    let routing_key = to_routing_key(&row.event_type);
    tracing::info!(
        outbox_id = %row.id,
        event_type = %row.event_type,
        routing_key = %routing_key,
        "Relay: publishing event envelope"
    );

    // Standardized Event Envelope
    let envelope = serde_json::json!({
        "id": row.id,
        "event_type": row.event_type,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "payload": row.payload,
    });

    let body = serde_json::to_vec(&envelope)?;

    // Publish to Topic Exchange with standardized routing key
    let publish_result = ch
        .basic_publish(
            EXCHANGE_NAME,
            &routing_key,
            BasicPublishOptions::default(),
            &body,
            BasicProperties::default()
                .with_delivery_mode(2) // 2 = persistent (survives broker restart)
                .with_content_type("application/json".into())
                .with_message_id(row.id.to_string().into()),
        )
        .await;

    match publish_result {
        Ok(confirm) => {
            confirm.await?; // wait for broker ACK
            mark_done(db, row.id).await?;
            tracing::info!(outbox_id = %row.id, "Relay: event published and confirmed");
        }
        Err(e) => {
            schedule_retry(db, row.id, row.retry_count, &e.to_string()).await?;
        }
    }

    Ok(true)
}

// ── Status Helpers ────────────────────────────────────────────────────────────

async fn mark_done(db: &PgPool, id: Uuid) -> anyhow::Result<()> {
    sqlx::query!(
        "UPDATE outbox SET status = 'done', updated_at = NOW() WHERE id = $1",
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

async fn schedule_retry(
    db: &PgPool,
    id: Uuid,
    retry_count: i32,
    error: &str,
) -> anyhow::Result<()> {
    if retry_count >= MAX_RETRIES {
        tracing::error!(outbox_id = %id, retries = retry_count, error, "Relay: max retries exceeded");
        sqlx::query!(
            "UPDATE outbox SET status = 'failed', last_error = $2, updated_at = NOW() WHERE id = $1",
            id, error
        ).execute(db).await?;
    } else {
        tracing::warn!(outbox_id = %id, retry_count, error, "Relay: publish failed — scheduling retry");
        sqlx::query!(
            r#"
            UPDATE outbox
            SET    status        = 'queued',
                   retry_count   = retry_count + 1,
                   last_error    = $2,
                   process_after = NOW() + (INTERVAL '5 seconds' * POWER(2, retry_count)),
                   updated_at    = NOW()
            WHERE  id = $1
            "#,
            id,
            error
        )
        .execute(db)
        .await?;
    }
    Ok(())
}
