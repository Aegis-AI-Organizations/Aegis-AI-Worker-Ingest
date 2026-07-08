use aegis_ai_worker_ingest::domain::NetworkTopologyPayload;

#[test]
fn test_deserialize_topology() {
    let json_data = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "test-host",
                "ipAddresses": ["10.0.0.1"],
                "containers": [
                    {
                        "id": "c1",
                        "name": "test-container",
                        "image": "nginx:latest",
                        "processes": [
                            {
                                "pid": 123,
                                "name": "nginx",
                                "commandLine": "nginx -g daemon off;",
                                "user": "root"
                            }
                        ],
                        "ports": [
                            {
                                "number": 80,
                                "protocol": "tcp",
                                "state": "LISTEN"
                            }
                        ]
                    }
                ],
                "processes": []
            }
        ],
        "routes": []
    }"#;

    let payload: NetworkTopologyPayload = serde_json::from_str(json_data).unwrap();
    assert_eq!(payload.hosts[0].containers[0].name, "test-container");
}

#[test]
fn test_minio_notification_serde() {
    use aegis_ai_worker_ingest::domain::{
        MinioBucketDetails, MinioEventRecord, MinioNotification, MinioObjectDetails, MinioS3Details,
    };

    let record = MinioEventRecord {
        s3: MinioS3Details {
            bucket: MinioBucketDetails {
                name: "test-bucket".to_string(),
            },
            object: MinioObjectDetails {
                key: "test-key".to_string(),
            },
        },
    };

    let notification = MinioNotification {
        records: Some(vec![record.clone()]),
        key: Some("test-key".to_string()),
    };

    // Exercise Debug and Clone
    let debug_str = format!("{:?}", notification);
    assert!(debug_str.contains("test-bucket"));
    assert!(debug_str.contains("test-key"));

    let cloned = notification.clone();
    assert_eq!(cloned.key, Some("test-key".to_string()));

    let serialized = serde_json::to_string(&notification).unwrap();
    let deserialized: MinioNotification = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.key, Some("test-key".to_string()));
    assert!(deserialized.records.is_some());
    assert_eq!(
        deserialized.records.unwrap()[0].s3.bucket.name,
        "test-bucket"
    );
}

#[test]
fn test_topology_derived_traits() {
    use aegis_ai_worker_ingest::domain::{
        NetworkTopologyPayload, ProtoContainer, ProtoHost, ProtoPort, ProtoProcess, ProtoRoute,
    };

    let port = ProtoPort {
        number: 80,
        protocol: "tcp".to_string(),
        state: Some("LISTEN".to_string()),
        host_ip: Some("0.0.0.0".to_string()),
        host_port: Some(8080),
        source: Some("docker_port_bindings".to_string()),
    };

    let process = ProtoProcess {
        pid: 123,
        name: "nginx".to_string(),
        command_line: Some("nginx".to_string()),
        user: Some("root".to_string()),
    };

    let container = ProtoContainer {
        id: "c1".to_string(),
        name: "nginx-container".to_string(),
        image: "nginx:latest".to_string(),
        image_version: Some("nginx:1.21.0-alpine".to_string()),
        image_hash: Some("sha256:test".to_string()),
        image_sha256: Some("sha256:test".to_string()),
        image_archive_ref: None,
        image_archive_object: None,
        env: std::collections::BTreeMap::from([(String::from("FOO"), String::from("bar"))]),
        labels: std::collections::BTreeMap::new(),
        networks: Vec::new(),
        processes: vec![process.clone()],
        ports: vec![port.clone()],
        exposed_ports: vec![port.clone()],
        privileged: Some(false),
        run_as_root: Some(true),
        sensitive_volumes: vec!["/var/run/secrets".to_string()],
    };

    let host = ProtoHost {
        id: "h1".to_string(),
        hostname: "test-host".to_string(),
        ip_addresses: vec!["10.0.0.1".to_string()],
        containers: vec![container.clone()],
        processes: vec![process.clone()],
    };

    let payload = NetworkTopologyPayload {
        hosts: vec![host.clone()],
        routes: vec![ProtoRoute {
            kind: "k8s_service".to_string(),
            source_kind: "service".to_string(),
            source_name: "web".to_string(),
            source_namespace: Some("default".to_string()),
            target_kind: "service".to_string(),
            target_name: "web".to_string(),
            target_namespace: Some("default".to_string()),
            host: Some("example.com".to_string()),
            path: Some("/".to_string()),
            path_type: Some("Prefix".to_string()),
            protocol: "tcp".to_string(),
            source_port: Some(443),
            target_port: Some("http".to_string()),
            published_port: Some(443),
        }],
        database_schemas: Vec::new(),
    };

    // Exercise Debug and Clone
    let debug_str = format!("{:?}", payload);
    assert!(debug_str.contains("test-host"));
    assert!(debug_str.contains("nginx-container"));

    let cloned = payload.clone();
    assert_eq!(cloned.hosts.len(), 1);
    assert_eq!(cloned.hosts[0].hostname, "test-host");

    let serialized = serde_json::to_string(&payload).unwrap();
    let deserialized: NetworkTopologyPayload = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.hosts[0].hostname, "test-host");
    assert_eq!(deserialized.hosts[0].containers[0].ports[0].number, 80);
    assert_eq!(deserialized.routes[0].kind, "k8s_service");
}

#[tokio::test]
async fn test_download_topology_file_success() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("GET", "/test-bucket/some-key")
        .with_status(200)
        .with_body("some UTF-8 content")
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: server.url(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: "".to_string(),
        neo4j_auth: "".to_string(),
    });

    let res = activities
        .download_topology_file_impl("some-key".to_string())
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "some UTF-8 content");
}

#[tokio::test]
async fn test_download_topology_file_s3_error() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("GET", "/test-bucket/some-key")
        .with_status(404)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: server.url(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: "".to_string(),
        neo4j_auth: "".to_string(),
    });

    let res = activities
        .download_topology_file_impl("some-key".to_string())
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_download_topology_file_utf8_error() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("GET", "/test-bucket/some-key")
        .with_status(200)
        .with_body(vec![0, 159, 146, 150]) // Invalid UTF-8 bytes
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: server.url(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: "".to_string(),
        neo4j_auth: "".to_string(),
    });

    let res = activities
        .download_topology_file_impl("some-key".to_string())
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_write_telemetry_to_clickhouse_success() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .create_async()
        .await;

    let clickhouse_client = clickhouse::Client::default()
        .with_url(server.url())
        .with_database("test_db");

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client,
        neo4j_url: "".to_string(),
        neo4j_auth: "".to_string(),
    });

    let payload_json = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "host1",
                "ipAddresses": ["10.0.0.1"],
                "containers": [
                    {
                        "id": "c1",
                        "name": "cont1",
                        "image": "img1",
                        "processes": [
                            {
                                "pid": 111,
                                "name": "cproc",
                                "commandLine": "ccmd",
                                "user": "cuser"
                            }
                        ],
                        "ports": [
                            {
                                "number": 80,
                                "protocol": "tcp",
                                "state": "LISTEN"
                            }
                        ]
                    }
                ],
                "processes": [
                    {
                        "pid": 456,
                        "name": "proc1",
                        "commandLine": "cmd1",
                        "user": "usr1"
                    }
                ]
            }
        ]
    }"#
    .to_string();

    let res = activities
        .write_telemetry_to_clickhouse_impl(
            payload_json,
            "agent-1".to_string(),
            "company-1".to_string(),
        )
        .await;

    assert!(res.is_ok());
}

#[tokio::test]
async fn test_write_telemetry_to_clickhouse_invalid_json() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: "".to_string(),
        neo4j_auth: "".to_string(),
    });

    let res = activities
        .write_telemetry_to_clickhouse_impl(
            "invalid json".to_string(),
            "agent-1".to_string(),
            "company-1".to_string(),
        )
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_success() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("UNWIND \\$hosts AS host MERGE".to_string()),
            mockito::Matcher::Regex("UNWIND \\$containers AS container MERGE".to_string()),
            mockito::Matcher::Regex("\"env\":\\[\\]".to_string()),
            mockito::Matcher::Regex(
                "\"ports\":\\[\"80:tcp:LISTEN:::k8s_container\"\\]".to_string(),
            ),
            mockito::Matcher::Regex(
                "\"exposedPorts\":\\[\"80:tcp:LISTEN:::k8s_container\"\\]".to_string(),
            ),
            mockito::Matcher::Regex("UNWIND \\$processes AS process MERGE".to_string()),
            mockito::Matcher::Regex("UNWIND \\$routes AS route MERGE".to_string()),
            mockito::Matcher::Regex("UNWIND \\$endpoints AS endpoint MERGE".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"errors": []}"#)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "host1",
                "ipAddresses": ["10.0.0.1"],
                "containers": [
                    {
                        "id": "c1",
                        "name": "cont1",
                        "image": "img1",
                        "processes": [
                            {
                                "pid": 789,
                                "name": "proc2",
                                "commandLine": "cmd2",
                                "user": "usr2"
                            }
                        ],
                        "ports": [
                            {
                                "number": 80,
                                "protocol": "tcp",
                                "state": "LISTEN",
                                "source": "k8s_container"
                            }
                        ],
                        "exposedPorts": [
                            {
                                "number": 80,
                                "protocol": "tcp",
                                "state": "LISTEN",
                                "source": "k8s_container"
                            }
                        ]
                    }
                ],
                "processes": []
            }
        ],
        "routes": [
            {
                "kind": "k8s_service",
                "source_kind": "service",
                "source_name": "web",
                "source_namespace": "default",
                "target_kind": "service",
                "target_name": "web",
                "target_namespace": "default",
                "host": "example.com",
                "path": "/",
                "path_type": "Prefix",
                "protocol": "tcp",
                "source_port": 443,
                "target_port": "http",
                "published_port": 443
            }
        ]
    }"#
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_ok());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_persists_container_metadata_relations() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use serde_json::json;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("CONNECTED_TO".to_string()),
            mockito::Matcher::Regex("DEPENDS_ON".to_string()),
            mockito::Matcher::Regex(
                "\"networkId\":\"company-1:agent-1:network:frontend\"".to_string(),
            ),
            mockito::Matcher::Regex("\"sourceId\":\"company-1:agent-1:api\"".to_string()),
            mockito::Matcher::Regex("\"targetId\":\"company-1:agent-1:postgres\"".to_string()),
            mockito::Matcher::Regex(
                "\"env\":\\[\"DATABASE_HOST=postgres://Postgres:5432/app\"\\]".to_string(),
            ),
            mockito::Matcher::Regex("\"labels\":\\[\"tier=backend\"\\]".to_string()),
            mockito::Matcher::Regex(
                "\"ports\":\\[\"8080:tcp:LISTEN:127.0.0.1:18080:docker_port_bindings\"\\]"
                    .to_string(),
            ),
        ]))
        .with_status(200)
        .with_body(r#"{"errors": []}"#)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = json!({
        "hosts": [{
            "id": "h1",
            "hostname": "host1",
            "ipAddresses": ["10.0.0.1"],
            "containers": [
                {
                    "id": "api",
                    "name": "api",
                    "image": "api:latest",
                    "env": {
                        "DATABASE_HOST": "postgres://Postgres:5432/app"
                    },
                    "labels": {
                        "tier": "backend"
                    },
                    "networks": ["frontend"],
                    "processes": [],
                    "ports": [{
                        "number": 8080,
                        "protocol": "tcp",
                        "state": "LISTEN",
                        "hostIp": "127.0.0.1",
                        "hostPort": 18080,
                        "source": "docker_port_bindings"
                    }],
                    "exposedPorts": [],
                    "privileged": false,
                    "runAsRoot": false,
                    "sensitiveVolumes": []
                },
                {
                    "id": "postgres",
                    "name": "postgres",
                    "image": "postgres:16",
                    "env": {},
                    "labels": {},
                    "networks": ["frontend"],
                    "processes": [],
                    "ports": [{
                        "number": 5432,
                        "protocol": "tcp",
                        "state": "LISTEN",
                        "hostIp": "127.0.0.1",
                        "hostPort": 15432,
                        "source": "docker_port_bindings"
                    }],
                    "exposedPorts": [],
                    "privileged": false,
                    "runAsRoot": false,
                    "sensitiveVolumes": []
                }
            ],
            "processes": []
        }],
        "routes": []
    })
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_ok());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_persists_database_schema_relations() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use serde_json::json;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("DatabaseSchema".to_string()),
            mockito::Matcher::Regex("USES_DATABASE".to_string()),
            mockito::Matcher::Regex("\"engine\":\"postgresql\"".to_string()),
            mockito::Matcher::Regex("\"databaseName\":\"app_db\"".to_string()),
            mockito::Matcher::Regex("\"containerId\":\"company-1:agent-1:api\"".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"errors": []}"#)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = json!({
        "hosts": [{
            "id": "h1",
            "hostname": "host1",
            "ipAddresses": ["10.0.0.1"],
            "containers": [{
                "id": "api",
                "name": "api",
                "image": "api:latest",
                "processes": [],
                "ports": [],
                "exposedPorts": []
            }],
            "processes": []
        }],
        "routes": [],
        "databaseSchemas": [{
            "engine": "postgresql",
            "host": "postgres.default.svc",
            "port": 5432,
            "databaseName": "app_db",
            "username": "app_user",
            "sourceContainerId": "api",
            "sourceContainerName": "api",
            "tables": []
        }]
    })
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_ok());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_batches_1000_hosts_in_one_transaction() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use serde_json::json;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("UNWIND \\$hosts AS host MERGE".to_string()),
            mockito::Matcher::Regex("\"id\":\"company-1:agent-1:h0\"".to_string()),
            mockito::Matcher::Regex("\"id\":\"company-1:agent-1:h999\"".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"errors": []}"#)
        .expect(1)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();
    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let hosts = (0..1000)
        .map(|index| {
            json!({
                "id": format!("h{}", index),
                "hostname": format!("host-{}", index),
                "ipAddresses": ["10.0.0.1"],
                "containers": [],
                "processes": []
            })
        })
        .collect::<Vec<_>>();

    let res = activities
        .write_graph_to_neo4j_impl(
            json!({ "hosts": hosts }).to_string(),
            "agent-1".to_string(),
            "company-1".to_string(),
        )
        .await;

    assert!(res.is_ok());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_invalid_json() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: "".to_string(),
        neo4j_auth: "".to_string(),
    });

    let res = activities
        .write_graph_to_neo4j_impl(
            "invalid json".to_string(),
            "agent-1".to_string(),
            "company-1".to_string(),
        )
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_http_error() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .with_status(500)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "host1",
                "ipAddresses": ["10.0.0.1"],
                "containers": [],
                "processes": []
            }
        ]
    }"#
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_execution_error() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .with_status(200)
        .with_body(r#"{"errors": [{"code": "Neo.ClientError.Statement.SyntaxError", "message": "Invalid Cypher syntax"}]}"#)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "host1",
                "ipAddresses": ["10.0.0.1"],
                "containers": [],
                "processes": []
            }
        ]
    }"#
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_writes_host_process_relationships() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::Regex("MERGE \\(p:Process".to_string()),
            mockito::Matcher::Regex("RUNS_PROCESS".to_string()),
            mockito::Matcher::Regex("\"processId\":\"company-1:agent-1:h1-proc-456\"".to_string()),
        ]))
        .with_status(200)
        .with_body(r#"{"errors":[]}"#)
        .expect(1)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "host1",
                "ipAddresses": [],
                "containers": [],
                "processes": [{
                    "pid": 456,
                    "name": "agent",
                    "commandLine": "/usr/bin/agent",
                    "user": "root"
                }]
            }
        ]
    }"#
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_ok());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_invalid_response_json() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/db/neo4j/tx/commit")
        .with_status(200)
        .with_body("invalid response")
        .expect(1)
        .create_async()
        .await;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: server.url(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let payload_json = r#"{
        "hosts": [
            {
                "id": "h1",
                "hostname": "host1",
                "ipAddresses": [],
                "containers": [],
                "processes": []
            }
        ]
    }"#
    .to_string();

    let res = activities
        .write_graph_to_neo4j_impl(payload_json, "agent-1".to_string(), "company-1".to_string())
        .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_write_graph_to_neo4j_empty_topology_is_a_no_op() {
    use aegis_ai_worker_ingest::activities::IngestActivities;
    use std::sync::Arc;

    let s3_region = s3::region::Region::Custom {
        region: "us-east-1".to_owned(),
        endpoint: "http://127.0.0.1:9000".to_owned(),
    };
    let s3_credentials =
        s3::creds::Credentials::new(Some("access"), Some("secret"), None, None, None).unwrap();
    let minio_bucket = s3::Bucket::new("test-bucket", s3_region, s3_credentials)
        .unwrap()
        .with_path_style();

    let activities = Arc::new(IngestActivities {
        minio_bucket,
        clickhouse_client: clickhouse::Client::default(),
        neo4j_url: "http://unused.invalid".to_string(),
        neo4j_auth: "Basic dGVzdDp0ZXN0".to_string(),
    });

    let res = activities
        .write_graph_to_neo4j_impl(
            r#"{"hosts":[]}"#.to_string(),
            "agent-1".to_string(),
            "company-1".to_string(),
        )
        .await;

    assert!(res.is_ok());
}
