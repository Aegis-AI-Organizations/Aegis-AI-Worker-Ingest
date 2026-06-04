use aegis_ai_worker_ingest::{activities, ingest, workflows};

use std::fs;
use temporalio_client::{
    Client, ClientOptions, ClientTlsOptions, Connection, ConnectionOptions, TlsOptions,
};
use temporalio_sdk::{Worker, WorkerOptions};
use temporalio_sdk_core::{CoreRuntime, RuntimeOptions, Url};

fn startup_banner() -> &'static str {
    "Hello from Aegis AI Worker Ingest!"
}

async fn run() {
    println!("{}", startup_banner());
    println!("{}", ingest::startup_message());

    if std::env::var("TEST_MODE").is_ok() {
        println!("Test mode detected, exiting run()");
        return;
    }

    // clickhouse config
    let clickhouse_host =
        std::env::var("CLICKHOUSE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let clickhouse_port = std::env::var("CLICKHOUSE_PORT").unwrap_or_else(|_| "8123".to_string());
    let clickhouse_user =
        std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".to_string());
    let clickhouse_password = std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default();
    let clickhouse_db = std::env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "default".to_string());

    let clickhouse_url = format!("http://{}:{}", clickhouse_host, clickhouse_port);
    println!(
        "Configuring ClickHouse client for {}:{} database {}",
        clickhouse_host, clickhouse_port, clickhouse_db
    );
    let clickhouse_client = clickhouse::Client::default()
        .with_url(clickhouse_url)
        .with_user(clickhouse_user)
        .with_password(clickhouse_password)
        .with_database(clickhouse_db);

    // Initialize clickhouse table
    println!("Ensuring ClickHouse system_events table exists...");
    if let Err(e) = init_clickhouse(&clickhouse_client).await {
        eprintln!("Critical: ClickHouse initialization failed: {}", e);
    }

    // --- Temporal Client & Worker Setup ---
    let temporal_host =
        std::env::var("TEMPORAL_HOST").unwrap_or_else(|_| "localhost:7233".to_string());
    let temporal_namespace =
        std::env::var("TEMPORAL_NAMESPACE").unwrap_or_else(|_| "default".to_string());
    let temporal_tls_enabled = std::env::var("TEMPORAL_TLS_ENABLE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    // Connect to Temporal
    let temp_url_str = if temporal_host.contains("://") {
        temporal_host.clone()
    } else if temporal_tls_enabled {
        format!("https://{}", temporal_host)
    } else {
        format!("http://{}", temporal_host)
    };
    let temp_url =
        Url::parse(&temp_url_str).unwrap_or_else(|_| Url::parse("http://localhost:7233").unwrap());

    println!("Connecting to Temporal at {}...", temp_url_str);
    println!(
        "Temporal config namespace={}, queue=INGEST_TASK_QUEUE",
        temporal_namespace
    );

    // We create the runtime
    let runtime_options = RuntimeOptions::builder().build().unwrap();
    let runtime = CoreRuntime::new_assume_tokio(runtime_options).unwrap();

    let tls_options = if temporal_tls_enabled {
        Some(build_temporal_tls_options().expect("Failed to build Temporal TLS options"))
    } else {
        None
    };
    let connection_options = ConnectionOptions::new(temp_url)
        .maybe_tls_options(tls_options)
        .build();
    let connection = match Connection::connect(connection_options).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect to Temporal: {}", e);
            return;
        }
    };

    let temporal_client = Client::new(connection, ClientOptions::new(temporal_namespace).build())
        .expect("Failed to create Temporal client");

    // MinIO Client setup for activities
    let minio_endpoint =
        std::env::var("MINIO_ENDPOINT").unwrap_or_else(|_| "localhost:9000".to_string());
    let minio_access_key = std::env::var("MINIO_ACCESS_KEY").unwrap_or_default();
    let minio_secret_key = std::env::var("MINIO_SECRET_KEY").unwrap_or_default();
    let minio_bucket_name =
        std::env::var("MINIO_BUCKET").unwrap_or_else(|_| "aegis-ingest".to_string());

    let s3_endpoint = if minio_endpoint.contains("://") {
        minio_endpoint.clone()
    } else {
        format!("http://{}", minio_endpoint)
    };

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: s3_endpoint.clone(),
    };
    let s3_credentials = s3::creds::Credentials::new(
        Some(&minio_access_key),
        Some(&minio_secret_key),
        None,
        None,
        None,
    )
    .unwrap();
    let minio_bucket = s3::Bucket::new(&minio_bucket_name, s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    // Neo4j config setup for activities
    let neo4j_url =
        std::env::var("NEO4J_URL").unwrap_or_else(|_| "http://localhost:7474".to_string());
    let neo4j_user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string());
    let neo4j_password =
        std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "neo4j_password".to_string());
    use base64::Engine;
    let auth_raw = format!("{}:{}", neo4j_user, neo4j_password);
    let auth_b64 = base64::engine::general_purpose::STANDARD.encode(auth_raw);
    let neo4j_auth = format!("Basic {}", auth_b64);

    // Run Temporal Worker on the main thread
    let worker_options = WorkerOptions::new("INGEST_TASK_QUEUE")
        .register_workflow::<workflows::IngestTopologyWorkflow>()
        .register_activities(activities::IngestActivities {
            minio_bucket,
            clickhouse_client: clickhouse_client.clone(),
            neo4j_url,
            neo4j_auth,
        })
        .build();

    println!("Registering Temporal worker activities and workflow for INGEST_TASK_QUEUE");
    let mut worker = Worker::new(&runtime, temporal_client, worker_options).unwrap();
    println!("Temporal Worker started on queue INGEST_TASK_QUEUE");
    if let Err(e) = worker.run().await {
        eprintln!("Temporal Worker error: {}", e);
    }
}

fn build_temporal_tls_options() -> anyhow::Result<TlsOptions> {
    let ca_path = std::env::var("TEMPORAL_TLS_CA_PATH").ok();
    let cert_path = std::env::var("TEMPORAL_TLS_CERT_PATH").ok();
    let key_path = std::env::var("TEMPORAL_TLS_KEY_PATH").ok();
    let server_name = std::env::var("TEMPORAL_TLS_SERVER_NAME").ok();

    let client_tls_options = match (cert_path, key_path) {
        (Some(cert_path), Some(key_path)) => Some(ClientTlsOptions {
            client_cert: fs::read(cert_path)?,
            client_private_key: fs::read(key_path)?,
        }),
        (None, None) => None,
        _ => anyhow::bail!(
            "TEMPORAL_TLS_CERT_PATH and TEMPORAL_TLS_KEY_PATH must be configured together"
        ),
    };

    Ok(TlsOptions {
        server_root_ca_cert: ca_path.map(fs::read).transpose()?,
        domain: server_name,
        client_tls_options,
    })
}

async fn init_clickhouse(client: &clickhouse::Client) -> anyhow::Result<()> {
    let ddl = "
        CREATE TABLE IF NOT EXISTS system_events (
            agent_id String,
            company_id String,
            event_type String,
            source String,
            message String,
            value Float64,
            timestamp DateTime
        ) ENGINE = MergeTree()
        ORDER BY (company_id, agent_id, event_type, timestamp, source)
    ";
    let migrations = [
        "ALTER TABLE system_events ADD COLUMN IF NOT EXISTS agent_id String",
        "ALTER TABLE system_events ADD COLUMN IF NOT EXISTS company_id String",
    ];

    let mut retries = 5;
    let mut attempt = 1;
    while retries > 0 {
        println!("ClickHouse init attempt {}...", attempt);
        match client.query(ddl).execute().await {
            Ok(_) => {
                for migration in migrations {
                    println!("Applying ClickHouse migration: {}", migration);
                    client.query(migration).execute().await?;
                }
                println!("Successfully initialized ClickHouse system_events table.");
                return Ok(());
            }
            Err(e) => {
                eprintln!(
                    "Failed to initialize ClickHouse on attempt {}: {}. Retrying in 2s...",
                    attempt, e
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                retries -= 1;
                attempt += 1;
            }
        }
    }
    Err(anyhow::anyhow!(
        "Could not initialize ClickHouse after several retries"
    ))
}

#[tokio::main]
async fn main() {
    run().await;
}

#[cfg(test)]
mod tests {
    use super::{run, startup_banner};
    use tokio::time::{Duration, timeout};

    #[test]
    fn startup_banner_matches_expected_message() {
        assert_eq!(startup_banner(), "Hello from Aegis AI Worker Ingest!");
    }

    #[tokio::test]
    async fn run_executes_without_panic() {
        unsafe {
            std::env::set_var("TEST_MODE", "true");
        }
        timeout(Duration::from_secs(1), run()).await.unwrap();
    }
}
