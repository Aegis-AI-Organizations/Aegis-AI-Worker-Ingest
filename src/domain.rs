use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// MinIO notification schema sent to Redis queue on upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinioEventRecord {
    pub s3: MinioS3Details,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinioS3Details {
    pub bucket: MinioBucketDetails,
    pub object: MinioObjectDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinioBucketDetails {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinioObjectDetails {
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinioNotification {
    #[serde(rename = "Records")]
    pub records: Option<Vec<MinioEventRecord>>,
    #[serde(rename = "Key")]
    pub key: Option<String>,
}

/// Agent-reported topology data payload structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopologyPayload {
    #[serde(default)]
    pub hosts: Vec<ProtoHost>,
    #[serde(default)]
    pub routes: Vec<ProtoRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoHost {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub ip_addresses: Vec<String>,
    #[serde(default)]
    pub containers: Vec<ProtoContainer>,
    #[serde(default)]
    pub processes: Vec<ProtoProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoContainer {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub image: String,
    #[serde(default, alias = "imageSha256")]
    pub image_sha256: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub networks: Vec<String>,
    #[serde(default)]
    pub processes: Vec<ProtoProcess>,
    #[serde(default)]
    pub ports: Vec<ProtoPort>,
    #[serde(default, alias = "exposedPorts")]
    pub exposed_ports: Vec<ProtoPort>,
    #[serde(default)]
    pub privileged: Option<bool>,
    #[serde(default, alias = "runAsRoot")]
    pub run_as_root: Option<bool>,
    #[serde(default, alias = "sensitiveVolumes")]
    pub sensitive_volumes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoProcess {
    #[serde(default)]
    pub pid: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "commandLine")]
    pub command_line: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoPort {
    #[serde(default)]
    pub number: i32,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, alias = "hostIp")]
    pub host_ip: Option<String>,
    #[serde(default, alias = "hostPort")]
    pub host_port: Option<i32>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoRoute {
    #[serde(default)]
    pub kind: String,
    #[serde(default, alias = "sourceKind")]
    pub source_kind: String,
    #[serde(default, alias = "sourceName")]
    pub source_name: String,
    #[serde(default, alias = "sourceNamespace")]
    pub source_namespace: Option<String>,
    #[serde(default, alias = "targetKind")]
    pub target_kind: String,
    #[serde(default, alias = "targetName")]
    pub target_name: String,
    #[serde(default, alias = "targetNamespace")]
    pub target_namespace: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, alias = "pathType")]
    pub path_type: Option<String>,
    #[serde(default)]
    pub protocol: String,
    #[serde(default, alias = "sourcePort")]
    pub source_port: Option<i32>,
    #[serde(default, alias = "targetPort")]
    pub target_port: Option<String>,
    #[serde(default, alias = "publishedPort")]
    pub published_port: Option<i32>,
}
