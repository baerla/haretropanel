# 1. Build stage
FROM rust:1.96 AS builder

WORKDIR /app

# Cache-friendly dependency build: copy manifest first
COPY Cargo.toml Cargo.lock* ./
COPY templates ./templates
COPY src ./src

# Release build
RUN cargo build --release

# 2. Runtime stage - minimal Linux
FROM debian:bookworm-slim

# Add CA certificates for HTTPS (HA, etc.)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and templates
COPY --from=builder /app/target/release/haretropanel /app/haretropanel
COPY templates ./templates

# Default env
ENV HARETROPANEL_PORT=8080

EXPOSE 8080

CMD ["./haretropanel"]