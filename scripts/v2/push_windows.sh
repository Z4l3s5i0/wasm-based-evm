#!/bin/bash

# Wrapper script to run the PowerShell push_windows.ps1 script from Bash/WSL
# This allows uniform usage across different platforms

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Check if powershell.exe is available (typically in WSL)
if command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -ExecutionPolicy Bypass -File "$(wslpath -w "$SCRIPT_DIR/push_windows.ps1")" "$@"
else
    echo "Error: powershell.exe not found. This script requires PowerShell to build Windows containers."
    exit 1
fi
