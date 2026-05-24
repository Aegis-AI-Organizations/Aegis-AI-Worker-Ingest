use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::SystemTime;
use temporalio_macros::activities;
use temporalio_sdk::activities::{ActivityContext, ActivityError};

use crate::domain::NetworkTopologyPayload;
use crate::ingest::ClickHouseEventRow;

pub struct IngestActivities {
    pub minio_bucket: s3::Bucket,
    pub clickhouse_client: clickhouse::Client,
    pub neo4j_url: String,
    pub neo4j_auth: String, // "Basic <credentials>"
}

#[derive(Deserialize)]
struct Neo4jError {
    code: String,
    message: String,
}

#[derive(Deserialize)]
struct Neo4jTxResponse {
    errors: Vec<Neo4jError>,
}

#[derive(Serialize)]
struct Neo4jStatement {
    statement: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct Neo4jTxRequest {
    statements: Vec<Neo4jStatement>,
}

#[activities]
impl IngestActivities {
    #[activity]
    pub async fn download_topology_file(
        self: Arc<Self>,
        _ctx: ActivityContext,
        _bucket: String,
        key: String,
    ) -> Result<String, ActivityError> {
        let response = self
            .minio_bucket
            .get_object(&key)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download file from MinIO: {}", e))?;

        let content = String::from_utf8(response.to_vec())
            .map_err(|e| anyhow::anyhow!("Topology file is not valid UTF-8: {}", e))?;

        Ok(content)
    }

    #[activity]
    pub async fn write_telemetry_to_clickhouse(
        self: Arc<Self>,
        _ctx: ActivityContext,
        payload_json: String,
    ) -> Result<(), ActivityError> {
        let payload: NetworkTopologyPayload = serde_json::from_str(&payload_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse topology JSON: {}", e))?;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;

        let mut rows = Vec::new();

        for host in &payload.hosts {
            rows.push(ClickHouseEventRow {
                event_type: "Host".to_string(),
                source: host.hostname.clone(),
                message: format!("Host online (ip_addresses: {:?})", host.ip_addresses),
                value: 1.0,
                timestamp,
            });

            for container in &host.containers {
                rows.push(ClickHouseEventRow {
                    event_type: "Container".to_string(),
                    source: container.name.clone(),
                    message: format!("Container running (image: {})", container.image),
                    value: 1.0,
                    timestamp,
                });
            }

            for process in &host.processes {
                rows.push(ClickHouseEventRow {
                    event_type: "Process".to_string(),
                    source: process.name.clone(),
                    message: format!(
                        "Process running (pid: {}, command_line: {:?}, user: {:?})",
                        process.pid, process.command_line, process.user
                    ),
                    value: process.pid as f64,
                    timestamp,
                });
            }
        }

        if !rows.is_empty() {
            let mut insert = self
                .clickhouse_client
                .insert("system_events")
                .map_err(|e| {
                    anyhow::anyhow!("Failed to initialize ClickHouse insert query: {}", e)
                })?;
            for row in rows {
                insert
                    .write(&row)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to write row to ClickHouse: {}", e))?;
            }
            insert
                .end()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to finalize ClickHouse transaction: {}", e))?;
        }

        Ok(())
    }

    #[activity]
    pub async fn write_graph_to_neo4j(
        self: Arc<Self>,
        _ctx: ActivityContext,
        payload_json: String,
    ) -> Result<(), ActivityError> {
        let payload: NetworkTopologyPayload = serde_json::from_str(&payload_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse topology JSON: {}", e))?;

        let mut statements = Vec::new();

        for host in &payload.hosts {
            // MERGE Host node
            statements.push(Neo4jStatement {
                statement: "MERGE (h:Host {id: $id}) SET h.hostname = $hostname, h.ipAddresses = $ipAddresses".to_string(),
                parameters: json!({
                    "id": host.id,
                    "hostname": host.hostname,
                    "ipAddresses": host.ip_addresses,
                }),
            });

            // Processes running on Host directly
            for process in &host.processes {
                statements.push(Neo4jStatement {
                    statement: "MERGE (p:Process {id: $id}) SET p.name = $name, p.commandLine = $commandLine, p.user = $user, p.pid = $pid".to_string(),
                    parameters: json!({
                        "id": format!("{}-proc-{}", host.id, process.pid),
                        "pid": process.pid,
                        "name": process.name,
                        "commandLine": process.command_line,
                        "user": process.user,
                    }),
                });
                statements.push(Neo4jStatement {
                    statement: "MATCH (h:Host {id: $hostId}), (p:Process {id: $procId}) MERGE (h)-[:RUNS_PROCESS]->(p)".to_string(),
                    parameters: json!({
                        "hostId": host.id,
                        "procId": format!("{}-proc-{}", host.id, process.pid),
                    }),
                });
            }

            // Containers running on Host
            for container in &host.containers {
                statements.push(Neo4jStatement {
                    statement: "MERGE (c:Container {id: $id}) SET c.name = $name, c.image = $image"
                        .to_string(),
                    parameters: json!({
                        "id": container.id,
                        "name": container.name,
                        "image": container.image,
                    }),
                });
                statements.push(Neo4jStatement {
                    statement: "MATCH (h:Host {id: $hostId}), (c:Container {id: $containerId}) MERGE (h)-[:RUNS_CONTAINER]->(c)".to_string(),
                    parameters: json!({
                        "hostId": host.id,
                        "containerId": container.id,
                    }),
                });

                // Processes running inside Container
                for process in &container.processes {
                    statements.push(Neo4jStatement {
                        statement: "MERGE (p:Process {id: $id}) SET p.name = $name, p.commandLine = $commandLine, p.user = $user, p.pid = $pid".to_string(),
                        parameters: json!({
                            "id": format!("{}-proc-{}", container.id, process.pid),
                            "pid": process.pid,
                            "name": process.name,
                            "commandLine": process.command_line,
                            "user": process.user,
                        }),
                    });
                    statements.push(Neo4jStatement {
                        statement: "MATCH (c:Container {id: $containerId}), (p:Process {id: $procId}) MERGE (c)-[:RUNS_PROCESS]->(p)".to_string(),
                        parameters: json!({
                            "containerId": container.id,
                            "procId": format!("{}-proc-{}", container.id, process.pid),
                        }),
                    });
                }
            }
        }

        if !statements.is_empty() {
            let client = reqwest::Client::new();
            let url = format!("{}/db/neo4j/tx/commit", self.neo4j_url);
            let response = client
                .post(&url)
                .header("Authorization", &self.neo4j_auth)
                .json(&Neo4jTxRequest { statements })
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to send Cypher transaction to Neo4j: {}", e)
                })?;

            if !response.status().is_success() {
                return Err(
                    anyhow::anyhow!("Neo4j transaction HTTP error: {}", response.status()).into(),
                );
            }

            let tx_resp: Neo4jTxResponse = response
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse Neo4j response: {}", e))?;

            if !tx_resp.errors.is_empty() {
                let err_msgs: Vec<String> = tx_resp
                    .errors
                    .into_iter()
                    .map(|e| format!("{}: {}", e.code, e.message))
                    .collect();
                return Err(
                    anyhow::anyhow!("Neo4j execution errors: {}", err_msgs.join("; ")).into(),
                );
            }
        }

        Ok(())
    }
}
