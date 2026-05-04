#!/bin/bash
set -e

echo "Starting Grafana..."

# Check if docker is running
if ! systemctl is-active --quiet docker; then
    echo "Docker is not running. Starting Docker..."
    sudo systemctl start docker
fi

# Run Grafana container
docker run -d --name grafana -p 3000:3000 grafana/grafana:latest

echo "Waiting for Grafana to start..."
sleep 10

# Note: Automatic dashboard import via API would require more complex scripting (API keys, etc.)
# For now, we point the user to the dashboard files and instructions.

echo "Grafana is running at http://localhost:3000"
echo "Default credentials: admin / admin"
echo ""
echo "To import dashboards:"
echo "1. Go to Dashboards -> Import"
echo "2. Upload the following files from wasix-based-evm/grafana/dashboards/:"
echo "   - global-comparison.json"
echo "   - execution-efficiency.json"
echo "   - network-health.json"
echo "   - system-resources.json"
echo ""
echo "Make sure Prometheus is added as a data source at http://localhost:9090 (or host IP:9090)"
