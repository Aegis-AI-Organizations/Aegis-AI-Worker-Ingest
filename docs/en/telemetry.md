# 📊 Ingest Worker Telemetry

The Ingest Worker is itself a telemetry pipeline; these are the signals that
describe its own health.

---

## Recommended metrics

- Payloads received.
- Payloads rejected by schema or size.
- Processing latency (receipt → Redis, Redis → ClickHouse batch).
- Batch size and flush rate.
- Retry count.
- Redis and ClickHouse write failures.
- Queue depth / consumer lag.
- Throughput (events per second per core).

---

## Scaling visibility (KEDA)

The Ingest pool scales on Redis queue length. Track queue lag and cold-start
time, not just replica count; scale-to-zero applies only when the buffer is
empty.

---

## Logs

Logs include tenant-safe identifiers and object keys — never raw payload content,
unless explicitly enabled in a local debug environment. mTLS client identity is
logged for auditing; certificates and keys are not.

---

*Aegis AI Telemetry & Data Engineering — 2026*
