#!/bin/bash
set -e

# Log Aggregator for Evaluation
# This script merges split log files from log_processor.sh into single files per service.
# Usage: ./aggregate_logs.sh <log_directory> [output_directory]

LOG_DIR=$1
OUTPUT_DIR=${2:-"./aggregated_logs"}

if [ -z "$LOG_DIR" ]; then
    echo "Usage: $0 <log_directory> [output_directory]"
    echo "Example: $0 ./logs_dump ./evaluation_logs"
    exit 1
fi

if [ ! -d "$LOG_DIR" ]; then
    echo "Error: Directory $LOG_DIR not found."
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

echo "Aggregating logs from $LOG_DIR into $OUTPUT_DIR..."

# The log files are named like: <container_name>_<chain_id>_<file_number>.log
# We want to group by container_name and chain_id, then sort by file_number and concatenate.

# Get all unique container_name_chain_id patterns
# We look for files ending in .log and extract the part before the last underscore and .log
PATTERNS=$(find "$LOG_DIR" -maxdepth 1 -name "*.log" -exec basename {} \; | sed -E 's/_[0-9]+\.log$//' | sort -u)

if [ -z "$PATTERNS" ]; then
    echo "No log files found in $LOG_DIR."
    exit 0
fi

for PATTERN in $PATTERNS; do
    echo "Processing $PATTERN..."
    
    # Create the output file
    OUT_FILE="$OUTPUT_DIR/${PATTERN}_aggregated.log"
    > "$OUT_FILE"
    
    # Find all files for this pattern, extract the number, sort numerically, and concatenate
    # Example file: el-node-0_12345_0.log
    
    # We use a temporary list to hold the files and their numbers for sorting
    TEMP_LIST=$(mktemp)
    
    for FILE in "$LOG_DIR/${PATTERN}"_*.log; do
        if [ -f "$FILE" ]; then
            # Extract the number before .log
            NUM=$(echo "$FILE" | grep -oP '(?<=_)\d+(?=\.log$)')
            echo "$NUM $FILE" >> "$TEMP_LIST"
        fi
    done
    
    # Sort numerically by the first column (the number) and then append to the output file
    sort -n "$TEMP_LIST" | awk '{print $2}' | xargs cat >> "$OUT_FILE"
    
    rm "$TEMP_LIST"
    
    LINE_COUNT=$(wc -l < "$OUT_FILE")
    echo "  -> Created $OUT_FILE ($LINE_COUNT lines)"
done

# Optional: Create a combined interleaved log file if timestamps are present
# Most logs in this system seem to have some form of timestamp.
# We can try to sort all lines from all aggregated files by their start.
# This might be resource intensive for very large logs, so we'll make it optional or a separate step.

echo ""
echo "Aggregation complete."
