FROM rust:1.88-slim AS builder
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# Copy dependency manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source and build dependencies to cache them
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Remove dummy build artifacts
RUN rm -f target/release/deps/aegis_ai_worker_ingest* target/release/aegis-ai-worker-ingest*

# Copy actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release && cp target/release/aegis-ai-worker-ingest /usr/local/bin/

# Stage 2: Minimal Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/aegis-ai-worker-ingest /usr/local/bin/
CMD ["aegis-ai-worker-ingest"]
