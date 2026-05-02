# 📥 Aegis AI — Ingest Worker

**Project ID:** AEGIS-CORE-2026

> The **Aegis AI Ingest Worker** is the high-throughput telemetry gateway of the platform. Written in **Rust**, it terminates mTLS streams from thousands of Agents, buffers them in Redis, and performs efficient batch-writes to ClickHouse for long-term security analysis.

---

## 🏗️ Role in the Ecosystem

The Ingest pool is responsible for the platform's "Data Intake" layer.

- **Stream Termination**: Handles bi-directional gRPC streams from `aegis-agents`.
- **Hot-Buffering**: Immediately pushes incoming events to **Redis** for real-time alerting and deduplication.
- **Micro-Batching**: Consolidates events into Optimized Batch Writes (~10,000 events) for **ClickHouse** ensuring massive ingestion performance.

```mermaid
graph LR
    Agents[Aegis Agents] -- "gRPC / mTLS" --> Nginx[Nginx Ingress]
    Nginx -- "Ingress" --> Ingest[Ingest Worker (Rust)]
    Ingest -- "Push" --> Redis[(Redis Hot-Buffer)]
    Ingest -- "Batch Write" --> ClickHouse[(ClickHouse OLAP)]
```

---

## 🛠️ Tech Stack & Performance

| Component | Technology | Version |
|---|---|---|
| Language | **Rust** | 1.75+ |
| Async Runtime | **Tokio** | 1.x |
| Persistence | **ClickHouse**, **Redis** | — |
| Throughput | 50,000 EPS / Node | — |

---

## 🔐 Security & DevSecOps

- **mTLS Mandatory**: Only accepts connections with certificates signed by the Aegis Internal CA.
- **Memory Safety**: Leveraging Rust to prevent buffer overflows and memory corruption in the high-intensity data path.
- **Resource Constraints**: Strictly limited via Kubernetes Cgroups to prevent ingestion spikes from affecting the orchestrator.

---

## 🐳 Deployment (Kubernetes)

Autoscaled by **KEDA** based on the Redis ingestion queue depth.

```yaml
# Helm values example
image:
  repository: ghcr.io/aegis-ai/aegis-worker-ingest
  tag: latest
keda:
  enabled: true
  minReplicas: 2
  maxReplicas: 30
  triggers:
    - type: redis
      metadata:
        listName: "ingest:queue"
        listLength: "5000"
```

---

## 🛠️ Development

```bash
# Build the binary
cargo build --release

# Run unit tests
cargo test
```

---

*Aegis AI — Telemetry & Data Engineering — 2026*
