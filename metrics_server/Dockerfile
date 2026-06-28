# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY target/release/metrics_server.exe /app/metrics_server.exe

# Copy the default config
COPY config /app/config

# Create data directory
RUN mkdir -p /app/data

# Expose the API port
EXPOSE 9100

# Set the entrypoint
ENTRYPOINT ["/app/metrics_server.exe"]
CMD ["run", "--config", "/app/config/docker.toml"]
