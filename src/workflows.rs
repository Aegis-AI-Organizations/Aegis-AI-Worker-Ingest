use std::time::Duration;
use temporalio_macros::{workflow, workflow_methods};
use temporalio_sdk::{ActivityOptions, WorkflowContext, WorkflowContextView, WorkflowResult};

use crate::activities::IngestActivities;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IngestArgs {
    pub bucket: String,
    pub key: String,
}

#[workflow]
#[derive(Debug, Default)]
pub struct IngestTopologyWorkflow {
    bucket: String,
    key: String,
}

#[workflow_methods]
impl IngestTopologyWorkflow {
    #[init]
    fn new(_ctx: &WorkflowContextView, args: IngestArgs) -> Self {
        Self {
            bucket: args.bucket,
            key: args.key,
        }
    }

    #[run]
    pub async fn run(ctx: &mut WorkflowContext<Self>) -> WorkflowResult<String> {
        let (bucket, key) = ctx.state(|s| (s.bucket.clone(), s.key.clone()));
        // Step 1: Download topology file from MinIO
        let download_opts = ActivityOptions::start_to_close_timeout(Duration::from_secs(60));
        let payload_json: String = ctx
            .start_activity(
                IngestActivities::download_topology_file,
                (bucket.clone(), key.clone()),
                download_opts,
            )
            .await?;

        // Step 2: Write telemetry to ClickHouse
        let clickhouse_opts = ActivityOptions::start_to_close_timeout(Duration::from_secs(30));
        ctx.start_activity(
            IngestActivities::write_telemetry_to_clickhouse,
            payload_json.clone(),
            clickhouse_opts,
        )
        .await?;

        // Step 3: Write topology to Neo4j
        let neo4j_opts = ActivityOptions::start_to_close_timeout(Duration::from_secs(60));
        ctx.start_activity(
            IngestActivities::write_graph_to_neo4j,
            payload_json,
            neo4j_opts,
        )
        .await?;

        Ok(format!(
            "Successfully ingested topology for file {}/{}",
            bucket, key
        ))
    }
}
