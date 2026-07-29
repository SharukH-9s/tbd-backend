//! Outbox Relay — reads `queued` rows from the `outbox` table and
//! publishes them to CloudAMQP (RabbitMQ). Runs as a background
//! `tokio::spawn` task started from `main.rs`.

use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    options::{BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
    types::FieldTable,
};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

const POLL_INTERVAL_SECS: u64 = 2;
const EXCHANGE_NAME: &str = "tbd.events";
const MAX_RETRIES: i32 = 5;

// ── Entry Point ───────────────────────────────────────────────────────────────

/// Start the relay loop. Called via `tokio::spawn(run_relay(db, amqp_url))` in main.rs.
pub async fn run_relay(db: PgPool, amqp_url: String) {
    tracing::info!("Relay task started — connecting to CloudAMQP...");

    loop {
        match connect_and_run(&db, &amqp_url).await {
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

async fn connect_and_run(db: &PgPool, amqp_url: &str) -> Result<(), lapin::Error> {
    let conn = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
    tracing::info!("Relay: connected to CloudAMQP");

    let channel = conn.create_channel().await?; // create a channel on the connection.
    setup_topology(&channel).await?;

    tracing::info!(
        "Relay: topology ready — polling outbox every {}s",
        POLL_INTERVAL_SECS
    );
    poll_loop(db, &channel).await;
    Ok(())
}

/// Declare the exchange and queues idempotently (durable = true).
/// RabbitMQ creates them on first run; subsequent runs are no-ops.
async fn setup_topology(ch: &Channel) -> Result<(), lapin::Error> {
    // creating the exchange which will route messages/events to the appropriate queues
    ch.exchange_declare(
        EXCHANGE_NAME,
        ExchangeKind::Fanout,
        ExchangeDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await?;

    // creating email_jobs queue — receives BookingCreated events. then binding to the exchange with "BookingCreated" key
    ch.queue_declare(
        "email_jobs", // name of the queue
        QueueDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await?;
    ch.queue_bind(
        "email_jobs",
        EXCHANGE_NAME,
        "", // Fanout exchange ignores the routing key
        QueueBindOptions::default(),
        FieldTable::default(),
    )
    .await?;

    // creating pdf_jobs queue — receives InvoiceRequested events. then binding to the exchange with "InvoiceRequested" key
    ch.queue_declare(
        "pdf_jobs", // name of the queue
        QueueDeclareOptions {
            durable: true,
            ..Default::default()
        },
        FieldTable::default(),
    )
    .await?;
    ch.queue_bind(
        "pdf_jobs",
        EXCHANGE_NAME,
        "", // Fanout exchange ignores the routing key
        QueueBindOptions::default(),
        FieldTable::default(),
    )
    .await?;

    tracing::info!("Relay: exchange '{}' and queues declared", EXCHANGE_NAME);
    Ok(())
}

// ── Poll Loop ─────────────────────────────────────────────────────────────────

async fn poll_loop(db: &PgPool, ch: &Channel) {
    let mut ticker = interval(Duration::from_secs(POLL_INTERVAL_SECS));
    loop {
        ticker.tick().await;

        if !ch.status().connected() {
            tracing::warn!("Relay: AMQP channel disconnected — exiting poll loop to reconnect");
            break;
        }

        if let Err(e) = process_one(db, ch).await {
            tracing::error!(error = %e, "Relay: error processing outbox row");
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

async fn process_one(db: &PgPool, ch: &Channel) -> anyhow::Result<()> {
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
        return Ok(()); // Nothing to process this tick
    };

    tracing::info!(outbox_id = %row.id, event_type = %row.event_type, "Relay: publishing event");

    let body = serde_json::to_vec(&row.payload)?;

    // publish the event/message to the exchange in RabbitMQ.
    // the exchange is "tbd.events", which is declared as a Fanout exchange.
    // RabbitMQ will broadcast this message to all bound queues ("email_jobs" and "pdf_jobs").
    let publish_result = ch
        .basic_publish(
            EXCHANGE_NAME,
            "", // routing key is ignored by Fanout exchanges
            BasicPublishOptions::default(),
            &body,
            BasicProperties::default()
                .with_delivery_mode(2) // 2 = persistent (survives broker restart)
                .with_content_type("application/json".into()),
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

    Ok(())
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
