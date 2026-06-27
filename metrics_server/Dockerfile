# Build stage
FROM rust:1.90-slim-bookworm AS builder

WORKDIR /usr/src/metrics_server

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy the Cargo files first to cache dependencies
COPY Cargo.toml ./
# Create a dummy src/main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy the actual source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/metrics_server/target/release/metrics_server /app/metrics_server

# Copy the default config
COPY config /app/config

# Create data directory
RUN mkdir -p /app/data

# Expose the API port
EXPOSE 9100

# Set the entrypoint
ENTRYPOINT ["/app/metrics_server"]
CMD ["run", "--config", "/app/config/docker.toml"]
