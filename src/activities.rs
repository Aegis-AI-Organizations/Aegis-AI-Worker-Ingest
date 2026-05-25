use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
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
        self.download_topology_file_impl(key).await
    }

    #[activity]
    pub async fn write_telemetry_to_clickhouse(
        self: Arc<Self>,
        _ctx: ActivityContext,
        payload_json: String,
    ) -> Result<(), ActivityError> {
        self.write_telemetry_to_clickhouse_impl(payload_json).await
    }

    #[activity]
    pub async fn write_graph_to_neo4j(
        self: Arc<Self>,
        _ctx: ActivityContext,
        payload_json: String,
    ) -> Result<(), ActivityError> {
        self.write_graph_to_neo4j_impl(payload_json).await
    }
}

impl IngestActivities {
    pub async fn download_topology_file_impl(&self, key: String) -> Result<String, ActivityError> {
        let response = self
            .minio_bucket
            .get_object(&key)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to download file from MinIO: {}", e))?;

        if !(200..300).contains(&response.status_code()) {
            return Err(anyhow::anyhow!(
                "Failed to download file from MinIO: HTTP {}",
                response.status_code()
            )
            .into());
        }

        let content = String::from_utf8(response.to_vec())
            .map_err(|e| anyhow::anyhow!("Topology file is not valid UTF-8: {}", e))?;

        Ok(content)
    }

    pub async fn write_telemetry_to_clickhouse_impl(
        &self,
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

    pub async fn write_graph_to_neo4j_impl(
        &self,
        payload_json: String,
    ) -> Result<(), ActivityError> {
        let payload: NetworkTopologyPayload = serde_json::from_str(&payload_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse topology JSON: {}", e))?;

        let started_at = Instant::now();
        let mut hosts = Vec::new();
        let mut containers = Vec::new();
        let mut processes = Vec::new();
        let mut host_containers = Vec::new();
        let mut host_processes = Vec::new();
        let mut container_processes = Vec::new();

        for host in &payload.hosts {
            hosts.push(json!({
                "id": host.id,
                "hostname": host.hostname,
                "ipAddresses": host.ip_addresses,
            }));

            for process in &host.processes {
                let process_id = format!("{}-proc-{}", host.id, process.pid);
                processes.push(json!({
                    "id": process_id,
                    "pid": process.pid,
                    "name": process.name,
                    "commandLine": process.command_line,
                    "user": process.user,
                }));
                host_processes.push(json!({
                    "hostId": host.id,
                    "processId": process_id,
                }));
            }

            for container in &host.containers {
                containers.push(json!({
                    "id": container.id,
                    "name": container.name,
                    "image": container.image,
                }));
                host_containers.push(json!({
                    "hostId": host.id,
                    "containerId": container.id,
                }));

                for process in &container.processes {
                    let process_id = format!("{}-proc-{}", container.id, process.pid);
                    processes.push(json!({
                        "id": process_id,
                        "pid": process.pid,
                        "name": process.name,
                        "commandLine": process.command_line,
                        "user": process.user,
                    }));
                    container_processes.push(json!({
                        "containerId": container.id,
                        "processId": process_id,
                    }));
                }
            }
        }

        let node_count = hosts.len() + containers.len() + processes.len();
        let mut statements = Vec::new();

        if !hosts.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $hosts AS host MERGE (h:Host {id: host.id}) SET h.hostname = host.hostname, h.ipAddresses = host.ipAddresses".to_string(),
                parameters: json!({ "hosts": hosts }),
            });
        }

        if !containers.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $containers AS container MERGE (c:Container {id: container.id}) SET c.name = container.name, c.image = container.image".to_string(),
                parameters: json!({ "containers": containers }),
            });
        }

        if !processes.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $processes AS process MERGE (p:Process {id: process.id}) SET p.name = process.name, p.commandLine = process.commandLine, p.user = process.user, p.pid = process.pid".to_string(),
                parameters: json!({ "processes": processes }),
            });
        }

        if !host_containers.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $relations AS relation MATCH (h:Host {id: relation.hostId}), (c:Container {id: relation.containerId}) MERGE (h)-[:RUNS_CONTAINER]->(c)".to_string(),
                parameters: json!({ "relations": host_containers }),
            });
        }

        if !host_processes.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $relations AS relation MATCH (h:Host {id: relation.hostId}), (p:Process {id: relation.processId}) MERGE (h)-[:RUNS_PROCESS]->(p)".to_string(),
                parameters: json!({ "relations": host_processes }),
            });
        }

        if !container_processes.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $relations AS relation MATCH (c:Container {id: relation.containerId}), (p:Process {id: relation.processId}) MERGE (c)-[:RUNS_PROCESS]->(p)".to_string(),
                parameters: json!({ "relations": container_processes }),
            });
        }

        if !statements.is_empty() {
            let statement_count = statements.len();
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

            println!(
                "Neo4j topology batch ingested: {} nodes in {} statements ({} ms)",
                node_count,
                statement_count,
                started_at.elapsed().as_millis()
            );
        }

        Ok(())
    }
}
