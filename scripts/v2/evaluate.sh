#!/bin/bash

# Wrapper script to run the log evaluation
# Usage: ./evaluate.sh [log_dir]

LOG_DIR=${1:-"./logs_dump"}

if [ ! -d "$LOG_DIR" ]; then
    echo "Error: Directory $LOG_DIR does not exist."
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_SCRIPT="$SCRIPT_DIR/evaluate_logs.py"

if command -v python3 &> /dev/null; then
    python3 "$PYTHON_SCRIPT" "$LOG_DIR"
elif command -v python &> /dev/null; then
    python "$PYTHON_SCRIPT" "$LOG_DIR"
else
    echo "Error: Python is not installed."
    exit 1
fi
