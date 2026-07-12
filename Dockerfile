# 1. Build stage
FROM rust:1.96 AS builder

WORKDIR /app

# Install Node.js for JS transpilation
RUN apt-get update && apt-get install -y curl ca-certificates
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
RUN apt-get install -y nodejs

# Cache-friendly dependency build: copy manifest first
COPY Cargo.toml Cargo.lock* ./
COPY templates ./templates
COPY src ./src
COPY package.json package-lock.json* ./
COPY babel.config.json ./
COPY src/js ./src/js

# Install JS deps and transpile
RUN npm install && make build

# 2. Runtime stage - minimal Linux
FROM debian:bookworm-slim

# Add CA certificates for HTTPS (HA, etc.)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and transpiled JS
COPY --from=builder /app/target/release/haretropanel /app/haretropanel
COPY --from=builder /app/public /app/public

# Default env
ENV HARETROPANEL_PORT=8080

EXPOSE 8080

CMD ["./haretropanel"]
