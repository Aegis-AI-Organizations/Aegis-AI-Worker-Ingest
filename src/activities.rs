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
        agent_id: String,
        company_id: String,
    ) -> Result<(), ActivityError> {
        self.write_telemetry_to_clickhouse_impl(payload_json, agent_id, company_id)
            .await
    }

    #[activity]
    pub async fn write_graph_to_neo4j(
        self: Arc<Self>,
        _ctx: ActivityContext,
        payload_json: String,
        agent_id: String,
        company_id: String,
    ) -> Result<(), ActivityError> {
        self.write_graph_to_neo4j_impl(payload_json, agent_id, company_id)
            .await
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
        agent_id: String,
        company_id: String,
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
                agent_id: agent_id.clone(),
                company_id: company_id.clone(),
                event_type: "Host".to_string(),
                source: host.hostname.clone(),
                message: format!("Host online (ip_addresses: {:?})", host.ip_addresses),
                value: 1.0,
                timestamp,
            });

            for container in &host.containers {
                rows.push(ClickHouseEventRow {
                    agent_id: agent_id.clone(),
                    company_id: company_id.clone(),
                    event_type: "Container".to_string(),
                    source: container.name.clone(),
                    message: format!(
                        "Container running (image: {}, image_sha256: {:?}, privileged: {:?}, run_as_root: {:?}, exposed_ports: {}, sensitive_volumes: {})",
                        container.image,
                        container.image_sha256,
                        container.privileged,
                        container.run_as_root,
                        container.exposed_ports.len(),
                        container.sensitive_volumes.len()
                    ),
                    value: 1.0,
                    timestamp,
                });
            }

            for process in &host.processes {
                rows.push(ClickHouseEventRow {
                    agent_id: agent_id.clone(),
                    company_id: company_id.clone(),
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

        for route in &payload.routes {
            rows.push(ClickHouseEventRow {
                agent_id: agent_id.clone(),
                company_id: company_id.clone(),
                event_type: "Route".to_string(),
                source: route.source_name.clone(),
                message: format!(
                    "Route discovered (kind: {}, source_kind: {}, target_kind: {}, protocol: {}, source_port: {:?}, target_port: {:?}, published_port: {:?})",
                    route.kind,
                    route.source_kind,
                    route.target_kind,
                    route.protocol,
                    route.source_port,
                    route.target_port,
                    route.published_port
                ),
                value: 1.0,
                timestamp,
            });
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
        agent_id: String,
        company_id: String,
    ) -> Result<(), ActivityError> {
        let payload: NetworkTopologyPayload = serde_json::from_str(&payload_json)
            .map_err(|e| anyhow::anyhow!("Failed to parse topology JSON: {}", e))?;

        let started_at = Instant::now();
        let mut hosts = Vec::new();
        let mut containers = Vec::new();
        let mut processes = Vec::new();
        let mut routes = Vec::new();
        let mut route_endpoints = Vec::new();
        let mut host_containers = Vec::new();
        let mut host_processes = Vec::new();
        let mut container_processes = Vec::new();
        let mut route_sources = Vec::new();
        let mut route_targets = Vec::new();

        for host in &payload.hosts {
            let host_id = scoped_topology_id(&company_id, &agent_id, &host.id);
            hosts.push(json!({
                "id": host_id,
                "rawId": host.id,
                "agentId": agent_id.clone(),
                "companyId": company_id.clone(),
                "hostname": host.hostname,
                "ipAddresses": host.ip_addresses,
            }));

            for process in &host.processes {
                let process_id = scoped_topology_id(
                    &company_id,
                    &agent_id,
                    &format!("{}-proc-{}", host.id, process.pid),
                );
                processes.push(json!({
                    "id": process_id,
                    "agentId": agent_id.clone(),
                    "companyId": company_id.clone(),
                    "pid": process.pid,
                    "name": process.name,
                    "commandLine": process.command_line,
                    "user": process.user,
                }));
                host_processes.push(json!({
                    "hostId": host_id,
                    "processId": process_id,
                }));
            }

            for container in &host.containers {
                let container_id = scoped_topology_id(&company_id, &agent_id, &container.id);
                containers.push(json!({
                    "id": container_id,
                    "rawId": container.id,
                    "agentId": agent_id.clone(),
                    "companyId": company_id.clone(),
                    "name": container.name,
                    "image": container.image,
                    "imageSha256": container.image_sha256,
                    "env": container.env,
                    "ports": container.ports,
                    "exposedPorts": container.exposed_ports,
                    "privileged": container.privileged,
                    "runAsRoot": container.run_as_root,
                    "sensitiveVolumes": container.sensitive_volumes,
                }));
                host_containers.push(json!({
                    "hostId": host_id,
                    "containerId": container_id,
                }));

                for process in &container.processes {
                    let process_id = scoped_topology_id(
                        &company_id,
                        &agent_id,
                        &format!("{}-proc-{}", container.id, process.pid),
                    );
                    processes.push(json!({
                        "id": process_id,
                        "agentId": agent_id.clone(),
                        "companyId": company_id.clone(),
                        "pid": process.pid,
                        "name": process.name,
                        "commandLine": process.command_line,
                        "user": process.user,
                    }));
                    container_processes.push(json!({
                        "containerId": container_id,
                        "processId": process_id,
                    }));
                }
            }
        }

        for route in &payload.routes {
            let route_raw_id = format!(
                "{}:{}:{}:{}:{}:{}:{}",
                route.kind,
                route.source_kind,
                route.source_name,
                route.target_kind,
                route.target_name,
                route.protocol,
                route
                    .published_port
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            let route_id = scoped_topology_id(&company_id, &agent_id, &route_raw_id);

            routes.push(json!({
                "id": route_id,
                "rawId": route_raw_id,
                "agentId": agent_id.clone(),
                "companyId": company_id.clone(),
                "kind": route.kind,
                "sourceKind": route.source_kind,
                "sourceName": route.source_name,
                "sourceNamespace": route.source_namespace,
                "targetKind": route.target_kind,
                "targetName": route.target_name,
                "targetNamespace": route.target_namespace,
                "host": route.host,
                "path": route.path,
                "pathType": route.path_type,
                "protocol": route.protocol,
                "sourcePort": route.source_port,
                "targetPort": route.target_port,
                "publishedPort": route.published_port,
            }));

            let source_raw_id = format!(
                "{}:{}:{}",
                route.source_kind,
                route.source_namespace.clone().unwrap_or_default(),
                route.source_name
            );
            let target_raw_id = format!(
                "{}:{}:{}",
                route.target_kind,
                route.target_namespace.clone().unwrap_or_default(),
                route.target_name
            );
            let source_id = scoped_topology_id(&company_id, &agent_id, &source_raw_id);
            let target_id = scoped_topology_id(&company_id, &agent_id, &target_raw_id);

            route_endpoints.push(json!({
                "id": source_id,
                "rawId": source_raw_id,
                "agentId": agent_id.clone(),
                "companyId": company_id.clone(),
                "kind": route.source_kind,
                "name": route.source_name,
                "namespace": route.source_namespace,
            }));
            route_endpoints.push(json!({
                "id": target_id,
                "rawId": target_raw_id,
                "agentId": agent_id.clone(),
                "companyId": company_id.clone(),
                "kind": route.target_kind,
                "name": route.target_name,
                "namespace": route.target_namespace,
            }));
            route_sources.push(json!({
                "routeId": route_id,
                "endpointId": source_id,
            }));
            route_targets.push(json!({
                "routeId": route_id,
                "endpointId": target_id,
            }));
        }

        let node_count =
            hosts.len() + containers.len() + processes.len() + routes.len() + route_endpoints.len();
        let mut statements = Vec::new();

        if !hosts.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $hosts AS host MERGE (h:Host {id: host.id}) SET h.rawId = host.rawId, h.agentId = host.agentId, h.companyId = host.companyId, h.hostname = host.hostname, h.ipAddresses = host.ipAddresses".to_string(),
                parameters: json!({ "hosts": hosts }),
            });
        }

        if !containers.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $containers AS container MERGE (c:Container {id: container.id}) SET c.rawId = container.rawId, c.agentId = container.agentId, c.companyId = container.companyId, c.name = container.name, c.image = container.image, c.imageSha256 = container.imageSha256, c.env = container.env, c.ports = container.ports, c.exposedPorts = container.exposedPorts, c.privileged = container.privileged, c.runAsRoot = container.runAsRoot, c.sensitiveVolumes = container.sensitiveVolumes".to_string(),
                parameters: json!({ "containers": containers }),
            });
        }

        if !processes.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $processes AS process MERGE (p:Process {id: process.id}) SET p.agentId = process.agentId, p.companyId = process.companyId, p.name = process.name, p.commandLine = process.commandLine, p.user = process.user, p.pid = process.pid".to_string(),
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

        if !routes.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $routes AS route MERGE (r:Route {id: route.id}) SET r.rawId = route.rawId, r.agentId = route.agentId, r.companyId = route.companyId, r.kind = route.kind, r.sourceKind = route.sourceKind, r.sourceName = route.sourceName, r.sourceNamespace = route.sourceNamespace, r.targetKind = route.targetKind, r.targetName = route.targetName, r.targetNamespace = route.targetNamespace, r.host = route.host, r.path = route.path, r.pathType = route.pathType, r.protocol = route.protocol, r.sourcePort = route.sourcePort, r.targetPort = route.targetPort, r.publishedPort = route.publishedPort".to_string(),
                parameters: json!({ "routes": routes }),
            });
        }

        if !route_endpoints.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $endpoints AS endpoint MERGE (e:RouteEndpoint {id: endpoint.id}) SET e.rawId = endpoint.rawId, e.agentId = endpoint.agentId, e.companyId = endpoint.companyId, e.kind = endpoint.kind, e.name = endpoint.name, e.namespace = endpoint.namespace".to_string(),
                parameters: json!({ "endpoints": route_endpoints }),
            });
        }

        if !route_sources.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $relations AS relation MATCH (r:Route {id: relation.routeId}), (e:RouteEndpoint {id: relation.endpointId}) MERGE (r)-[:ROUTE_FROM]->(e)".to_string(),
                parameters: json!({ "relations": route_sources }),
            });
        }

        if !route_targets.is_empty() {
            statements.push(Neo4jStatement {
                statement: "UNWIND $relations AS relation MATCH (r:Route {id: relation.routeId}), (e:RouteEndpoint {id: relation.endpointId}) MERGE (r)-[:ROUTE_TO]->(e)".to_string(),
                parameters: json!({ "relations": route_targets }),
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

fn scoped_topology_id(company_id: &str, agent_id: &str, raw_id: &str) -> String {
    format!("{}:{}:{}", company_id, agent_id, raw_id)
}
