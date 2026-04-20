# 📥 Ingest Architecture: High-Throughput Telemetry Gateway

The **Aegis AI Ingest Worker** is the high-performance "Ingestion Layer" of the platform. Written in **Rust** to handle massive concurrency with zero-copy efficiency, it serves as the entry point for all telemetry streamed from external Agents.

---

## 🏗️ Core Design Principles

The Ingest Worker is built for **reliability**, **throughput**, and **data integrity**:

1. **Memory Safety**: Leveraging Rust to prevent buffer overflows and race conditions in the high-intensity data path.
2. **Two-Stage Ingestion**:
   - **Hot Buffer (Redis)**: incoming events are immediately pushed to Redis for real-time alerting and deduplication.
   - **Batch Write (ClickHouse)**: Events are consolidated into optimized batches (~10,000 events) for massive OLAP ingestion performance in ClickHouse.
3. **mTLS Termination**: Acts as the secure terminus for bi-directional gRPC streams from thousands of Agents.

---

## 🔐 Secure Communication (mTLS)

The Ingest layer enforces strict **Mutual TLS (mTLS)** for all connections:

- **Certificate Validation**: The Ingest server verifies the Agent's client certificate against the Internal Root CA.
- **Bi-directional Trust**: Communication only proceeds if both the Agent and the Ingest server prove their cryptographic identity.
- **Identity Enforcement**: The worker validates the `Common Name` or `SAN` of the Agent to ensure it belongs to an authorized client workspace.

---

## 🌊 Dynamic Scaling (KEDA)

The Ingest pool is managed by **KEDA** (Kubernetes Event-Driven Autoscaling) to handle ingestion spikes:

- **Queue-Awareness**: KEDA polls the Redis ingestion queue length.
- **Rapid Scale-UP**: When queue size exceeds the safety threshold, KEDA spins up more Ingest workers to process the backlog.
- **Cost Efficiency**: Scales down to a minimal baseline when traffic is low.

```mermaid
graph LR
    Agents[Aegis Agents] -- "gRPC / mTLS" --> Nginx[Nginx Ingress]
    Nginx -- "Ingress" --> Ingest[Ingest Worker (Rust)]
    Ingest -- "Push" --> Redis[(Redis Hot-Buffer)]
    Ingest -- "Batch Write" --> ClickHouse[(ClickHouse OLAP)]
    Redis -- "Queue Length" --> KEDA[KEDA Operator]
    KEDA -- "Scale" --> Ingest
```

---

## ⚙️ Technical Specifications

- **Throughput Capability**: > 10,000 Events Per Second (EPS) per CPU core.
- **Serialization**: Protobuf-v3 (via Tonic/Prost).
- **Persistence**: ClickHouse (via `clickhouse-rs`) and Redis (via `redis-rs`).

---

*Aegis AI Telemetry & Data Engineering — 2026*
