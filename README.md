# 🦀 Aegis AI - Worker Pool: Ingest

**Project ID:** AEGIS-CORE-2026

## 🏗️ System Architecture & Role
The **Aegis AI Worker Ingest** sits at the direct junction of in-bound external telemetry and internal persistence. It processes the enormous throughput streamed from client `aegis-agents`.

* **Tech Stack:** Rust (Tokio for Runtime Concurrency, `clickhouse-rs`).
* **Role:**
  * Ingests mTLS-terminated agent streams routed via the Ingress controller.
  * Steps flow through a 2-stage mechanism:
    1. **Buffer:** Drops hot states directly into **Redis** for deduping and PUB/SUB triggers.
    2. **Batch Write:** Commits chunks of 10,000+ records to **ClickHouse** ensuring LZ4 compression efficiency.
* **Architecture Justification:** Rust provides memory-safe, zero-copy deserialization capable of ingesting massive traffic (10,000 EPS per node) without resource spikes.

## 🔐 Security & DevSecOps Mandates
* **Memory Integrity:** Bypasses garbage collection vulnerability vectors.
* **No Plain-Text Secrets:** Vault-injected runtime properties limit configuration attack paths.

## 🐳 Docker Deployment
Stateless workers autoscaled dynamically to manage ingestion queue depth.

```bash
docker pull ghcr.io/aegis-ai/aegis-worker-ingest:latest

infisical run --env=prod -- docker run -d \
  --name aegis-worker-ingest \
  --read-only \
  --cap-drop=ALL \
  --security-opt no-new-privileges:true \
  --user 10001:10001 \
  -e INFISICAL_TOKEN=$INFISICAL_TOKEN \
  ghcr.io/aegis-ai/aegis-worker-ingest:latest
```
