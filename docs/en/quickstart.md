# 📥 Quickstart: Ingest Worker

The Ingest Worker is a Rust service that terminates mTLS telemetry streams from
Agents, buffers events in Redis, and batch-writes them to ClickHouse.

---

## Prerequisites

- Rust 1.85+ (Tokio).
- Reachable Redis (hot buffer) and ClickHouse (OLAP store).
- mTLS material: internal CA plus a server certificate/key for the Ingest
  endpoint.

## Local development

```bash
cargo build
cargo test
cargo run
```

## Container build

```bash
docker build -t aegis-worker-ingest .
```

## Configuration checklist

- Redis and ClickHouse endpoints are reachable.
- mTLS certificate, key, and CA are mounted; client certificates are validated
  against the internal CA.
- Tenant context is derived from **trusted metadata** (validated client
  identity), never from unsigned payload content.
- Payload size and schema limits are configured.
- Batch size (~10k events) and flush interval are tuned for the target load.
- Retry behavior is idempotent — duplicate submissions must not double-write.

---

*Aegis AI Telemetry & Data Engineering — 2026*
