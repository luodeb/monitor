#!/bin/bash

# Output file name
OUTPUT_FILE="continuous_monitor.json"
STATE_DIR="$HOME/.continuous_monitor"
LAST_DMESG_TS_FILE="$STATE_DIR/last_dmesg_ts"
LAST_BOOT_ID_FILE="$STATE_DIR/last_boot_id"
mkdir -p "$STATE_DIR"

# Helper function to escape JSON strings safely
escape_json() {
    if command -v python3 &> /dev/null; then
        python3 -c "import json, sys; print(json.dumps(sys.stdin.read()).strip())"
    else
        local input=$(cat)
        local escaped=$(echo "$input" | sed 's/\\/\\\\/g' | sed 's/"/\\"/g' | awk '{printf "%s\\n", $0}' | sed 's/\\n$//')
        echo "\"$escaped\""
    fi
}

# Check for reboot
CURRENT_BOOT_ID=$(cat /proc/sys/kernel/random/boot_id 2>/dev/null || echo "unknown")
if [ -f "$LAST_BOOT_ID_FILE" ]; then
    LAST_BOOT_ID=$(cat "$LAST_BOOT_ID_FILE")
else
    LAST_BOOT_ID=""
fi

if [ "$CURRENT_BOOT_ID" != "$LAST_BOOT_ID" ]; then
    echo "System reboot detected (Boot ID changed). Resetting dmesg timestamp."
    echo "0" > "$LAST_DMESG_TS_FILE"
    echo "$CURRENT_BOOT_ID" > "$LAST_BOOT_ID_FILE"
fi

# Initialize last dmesg timestamp if not exists
if [ ! -f "$LAST_DMESG_TS_FILE" ]; then
    echo "0" > "$LAST_DMESG_TS_FILE"
fi

echo "Starting continuous monitoring (Ctrl+C to stop)..."

while true; do
    # 1. Hostname & IP
    HOSTNAME=$(hostname)
    IP_ADDRESS=$(hostname -I 2>/dev/null | awk '{print $1}')
    if [ -z "$IP_ADDRESS" ]; then
        IP_ADDRESS=$(ip route get 1 2>/dev/null | awk '{print $7; exit}')
    fi

    # 2. Top Info
    TOP_OUTPUT=$(top -b -n 1 | head -n 10)
    CPU_USAGE=$(echo "$TOP_OUTPUT" | grep "^%Cpu" | head -n 1)
    MEM_USAGE=$(echo "$TOP_OUTPUT" | grep "Mem :" | head -n 1)
    SWAP_USAGE=$(echo "$TOP_OUTPUT" | grep "Swap:" | head -n 1)

    # 3. Incremental Dmesg Logs
    LAST_DMESG_TS=$(cat "$LAST_DMESG_TS_FILE")
    
    # Get all logs (no color)
    DMESG_SOURCE=$(dmesg --color=never 2>/dev/null)
    
    TMP_MAX_FILE=$(mktemp)
    NEW_DMESG=$(echo "$DMESG_SOURCE" | awk -v last="$LAST_DMESG_TS" -v maxfile="$TMP_MAX_FILE" '
    BEGIN { max = last + 0 }
    {
      if ($0 ~ /^\[[[:space:]]*[0-9]+\.[0-9]+/) {
        line = $0
        sub(/^\[[[:space:]]*/, "", line)
        ts = line
        sub(/\].*/, "", ts)
        ts = ts + 0
        if (ts > max) {
          max = ts
        }
        if (ts > last) {
          print $0
        }
      }
    }
    END { printf "%.6f", max > maxfile }
    ')
    
    MAX_DMESG_TS=$(cat "$TMP_MAX_FILE")
    rm -f "$TMP_MAX_FILE"
    
    # Update timestamp only if we found newer logs
    if [ "$MAX_DMESG_TS" != "$LAST_DMESG_TS" ]; then
        echo "$MAX_DMESG_TS" > "$LAST_DMESG_TS_FILE"
    fi

    # Escape values
    JSON_HOSTNAME=$(echo -n "$HOSTNAME" | escape_json)
    JSON_IP=$(echo -n "$IP_ADDRESS" | escape_json)
    JSON_CPU=$(echo -n "$CPU_USAGE" | escape_json)
    JSON_MEM=$(echo -n "$MEM_USAGE" | escape_json)
    JSON_SWAP=$(echo -n "$SWAP_USAGE" | escape_json)
    JSON_DMESG=$(echo -n "$NEW_DMESG" | escape_json)
    TIMESTAMP=$(date +"%Y-%m-%dT%H:%M:%S%:z")

    # Construct JSON
    cat > "$OUTPUT_FILE" <<EOF
{
  "hostname": $JSON_HOSTNAME,
  "ip_address": $JSON_IP,
  "timestamp": "$TIMESTAMP",
  "system_metrics": {
    "cpu_info": $JSON_CPU,
    "memory_info": $JSON_MEM,
    "swap_info": $JSON_SWAP,
    "threadinfo": ""
  },
  "logs": {
    "dmesg": $JSON_DMESG
  }
}
EOF

    echo "[$(date '+%H:%M:%S')] Updated $OUTPUT_FILE"
    
    # Wait for 5 seconds
    sleep 5
done
