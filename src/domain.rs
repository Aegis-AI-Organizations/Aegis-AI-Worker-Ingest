use serde::{Deserialize, Serialize};

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
    pub hosts: Vec<ProtoHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoHost {
    pub id: String,
    pub hostname: String,
    pub ip_addresses: Vec<String>,
    pub containers: Vec<ProtoContainer>,
    pub processes: Vec<ProtoProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub processes: Vec<ProtoProcess>,
    pub ports: Vec<ProtoPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoProcess {
    pub pid: i32,
    pub name: String,
    pub command_line: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoPort {
    pub number: i32,
    pub protocol: String,
    pub state: Option<String>,
}
