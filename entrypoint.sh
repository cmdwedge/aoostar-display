#!/usr/bin/env bash
set -Eeo pipefail

log_info()  { echo "[$(date '+%Y-%m-%d %H:%M:%S')] [INFO]  $*"; }
log_warn()  { echo "[$(date '+%Y-%m-%d %H:%M:%S')] [WARN]  $*"; }
log_error() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] [ERROR] $*" >&2; }

SYSINFO_PID=""
ASTERCTL_PID=""
CUSTOM_PID=""

cleanup() {
    log_info "Received shutdown signal. Stopping services gracefully..."

    # Terminate custom scripts if running
    if [ -n "$CUSTOM_PID" ] && kill -0 "$CUSTOM_PID" 2>/dev/null; then
        log_info "Stopping custom sensor script (PID $CUSTOM_PID)..."
        kill -TERM "$CUSTOM_PID" 2>/dev/null || true
    fi

    # Terminate sysinfo provider
    if [ -n "$SYSINFO_PID" ] && kill -0 "$SYSINFO_PID" 2>/dev/null; then
        log_info "Stopping aster-sysinfo (PID $SYSINFO_PID)..."
        kill -TERM "$SYSINFO_PID" 2>/dev/null || true
    fi

    # Terminate asterctl
    if [ -n "$ASTERCTL_PID" ] && kill -0 "$ASTERCTL_PID" 2>/dev/null; then
        log_info "Stopping asterctl (PID $ASTERCTL_PID)..."
        kill -TERM "$ASTERCTL_PID" 2>/dev/null || true
    fi

    # Turn off display backlight on container shutdown if requested
    if [ "${AUTO_OFF_ON_STOP:-true}" = "true" ] && [ -n "${TARGET_DEVICE:-}" ] && [ -e "${TARGET_DEVICE}" ]; then
        log_info "Turning off LCD display backlight..."
        /usr/local/bin/asterctl --device "$TARGET_DEVICE" --off 2>/dev/null || true
    fi

    log_info "Shutdown complete. Exiting."
    exit 0
}

trap cleanup SIGTERM SIGINT SIGHUP

log_info "=== Starting Aoostar WTR Max Display Daemon ==="

# ------------------------------------------------------------------------------
# 0. Timezone Configuration & Auto-Detection
# ------------------------------------------------------------------------------
if [ -z "${TZ:-}" ] || [ "${TZ:-}" = "UTC" ]; then
    if [ -f /var/local/emhttp/var.ini ]; then
        DETECTED_TZ=$(grep -E '^timeZone=' /var/local/emhttp/var.ini 2>/dev/null | cut -d'=' -f2 | tr -d '"\r\n' || true)
        if [ -n "$DETECTED_TZ" ]; then
            export TZ="$DETECTED_TZ"
            log_info "Auto-detected host timezone from Unraid (/var/local/emhttp/var.ini): $TZ"
        fi
    elif [ -f /etc/timezone ]; then
        DETECTED_TZ=$(cat /etc/timezone 2>/dev/null | tr -d '\r\n' || true)
        if [ -n "$DETECTED_TZ" ]; then
            export TZ="$DETECTED_TZ"
            log_info "Auto-detected host timezone from /etc/timezone: $TZ"
        fi
    fi
fi

if [ -n "${TZ:-}" ] && [ -f "/usr/share/zoneinfo/$TZ" ]; then
    ln -sf "/usr/share/zoneinfo/$TZ" /etc/localtime 2>/dev/null || true
    log_info "Configured container timezone: $TZ (current local time: $(date '+%Y-%m-%d %H:%M:%S %Z'))"
fi

# ------------------------------------------------------------------------------
# 1. Initialize Configuration Directory (/config)
# ------------------------------------------------------------------------------
mkdir -p /config /tmp/sensors

if [ ! -f /config/monitor.json ]; then
    log_info "Seeding default configuration templates to /config..."
    cp -r /app/default-cfg/* /config/ 2>/dev/null || true
    chmod -R u+rwX,go+rX /config/ 2>/dev/null || true
elif grep -q "CPU温度" /config/monitor.json 2>/dev/null || grep -q "RAM Temperature" /config/monitor.json 2>/dev/null || ! grep -q "unraid_array_summary" /config/monitor.json 2>/dev/null || (grep -A 10 "Disk 1 Temp" /config/monitor.json 2>/dev/null | grep -q '"fontSize": 24'); then
    log_info "Detected outdated configuration in /config/monitor.json. Backing up to /config/monitor.json.bak and updating to latest layout..."
    cp /config/monitor.json /config/monitor.json.bak
    cp /app/default-cfg/monitor.json /config/monitor.json
fi

# Ensure latest background images and graphics are copied to /config
cp /app/default-cfg/*.jpg /config/ 2>/dev/null || true
cp /app/default-cfg/*.png /config/ 2>/dev/null || true

# Ensure sensor mappings exist in /config and update if legacy hardcoded mapping found
if [ ! -d /config/sensor-mapping ]; then
    mkdir -p /config/sensor-mapping
    cp -r /app/default-cfg/sensor-mapping/* /config/sensor-mapping/ 2>/dev/null || true
elif [ -f /config/sensor-mapping/sysinfo-to-aoostar.cfg ] && (grep -q "enp100s0f1np1" /config/sensor-mapping/sysinfo-to-aoostar.cfg 2>/dev/null || ! grep -q "unraid_array_summary" /config/sensor-mapping/sysinfo-to-aoostar.cfg 2>/dev/null); then
    log_info "Updating /config/sensor-mapping/sysinfo-to-aoostar.cfg with latest sensor mappings (backup saved to .bak)..."
    cp /config/sensor-mapping/sysinfo-to-aoostar.cfg /config/sensor-mapping/sysinfo-to-aoostar.cfg.bak
    cp /app/default-cfg/sensor-mapping/sysinfo-to-aoostar.cfg /config/sensor-mapping/sysinfo-to-aoostar.cfg
fi

# Ensure settings.json exists in /config
SETTINGS_FILE="/config/settings.json"
if [ ! -f "$SETTINGS_FILE" ]; then
    if [ -f /app/default-cfg/settings.json ]; then
        log_info "Seeding default settings.json to /config/settings.json..."
        cp /app/default-cfg/settings.json "$SETTINGS_FILE"
    fi
fi

# Apply any explicit environment variables to settings.json
if [ -f "$SETTINGS_FILE" ]; then
    if [ -n "${SLEEP_SCHEDULE_ENABLED:-}" ]; then
        sed -i "s/\"sleepScheduleEnabled\": [a-z]*/\"sleepScheduleEnabled\": ${SLEEP_SCHEDULE_ENABLED}/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${SLEEP_START_TIME:-}" ]; then
        sed -i "s/\"sleepStartTime\": \"[^\"]*\"/\"sleepStartTime\": \"${SLEEP_START_TIME}\"/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${SLEEP_END_TIME:-}" ]; then
        sed -i "s/\"sleepEndTime\": \"[^\"]*\"/\"sleepEndTime\": \"${SLEEP_END_TIME}\"/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${SLEEP_ON_STANDBY:-}" ]; then
        sed -i "s/\"sleepOnAllDisksStandby\": [a-z]*/\"sleepOnAllDisksStandby\": ${SLEEP_ON_STANDBY}/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${STANDBY_DELAY:-}" ]; then
        sed -i "s/\"standbyDelaySeconds\": [0-9]*/\"standbyDelaySeconds\": ${STANDBY_DELAY}/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${TEMP_UNIT:-}" ]; then
        sed -i "s/\"tempUnit\": \"[^\"]*\"/\"tempUnit\": \"${TEMP_UNIT}\"/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${SWITCH_TIME:-}" ]; then
        sed -i "s/\"switchTime\": [0-9]*/\"switchTime\": ${SWITCH_TIME}/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
    if [ -n "${REFRESH_INTERVAL:-}" ]; then
        sed -i "s/\"refreshInterval\": [0-9]*/\"refreshInterval\": ${REFRESH_INTERVAL}/g" "$SETTINGS_FILE" 2>/dev/null || true
    fi
fi

# ------------------------------------------------------------------------------
# 2. Detect Display Serial Device
# ------------------------------------------------------------------------------
TARGET_DEVICE="${DEVICE:-/dev/ttyACM0}"

if [ ! -e "$TARGET_DEVICE" ]; then
    log_warn "Configured device '$TARGET_DEVICE' not found. Searching for available USB serial devices..."
    
    DETECTED_DEVICE=""
    for dev in /dev/ttyACM* /dev/ttyUSB*; do
        if [ -e "$dev" ]; then
            DETECTED_DEVICE="$dev"
            break
        fi
    done

    if [ -n "$DETECTED_DEVICE" ]; then
        TARGET_DEVICE="$DETECTED_DEVICE"
        log_info "Auto-detected serial device: $TARGET_DEVICE"
    else
        log_warn "No /dev/ttyACM* or /dev/ttyUSB* device found!"
        log_warn "Ensure you passed the device to docker (e.g. '--device=/dev/ttyACM0')."
        log_warn "Will attempt fallback to USB VID:PID '${USB_ID:-0416:90A1}'..."
    fi
fi

# ------------------------------------------------------------------------------
# 3. Start System Sensor Metrics Collector (aster-sysinfo)
# ------------------------------------------------------------------------------
SENSOR_FILE="/tmp/sensors/values.txt"
touch "$SENSOR_FILE"

if [ "${ENABLE_SYSINFO:-true}" = "true" ]; then
    SYSINFO_ARGS=("--out" "$SENSOR_FILE" "--refresh" "${SENSOR_REFRESH:-2}")

    if [ -n "${DISK_REFRESH:-}" ] && [ "${DISK_REFRESH}" -gt 0 ] 2>/dev/null; then
        SYSINFO_ARGS+=("--disk-refresh" "$DISK_REFRESH")
    fi

    if [ "${ENABLE_SMART:-false}" = "true" ]; then
        SYSINFO_ARGS+=("--smartctl")
    fi

    if [ -f "$SETTINGS_FILE" ]; then
        SYSINFO_ARGS+=("--settings" "$SETTINGS_FILE")
    fi

    if [ -n "${NETWORK_INTERFACE:-}" ]; then
        SYSINFO_ARGS+=("--network-interface" "$NETWORK_INTERFACE")
    fi

    if [ -n "${TEMP_UNIT:-}" ]; then
        SYSINFO_ARGS+=("--temp-unit" "$TEMP_UNIT")
    fi

    log_info "Starting aster-sysinfo metrics collector (${SYSINFO_ARGS[*]})..."
    /usr/local/bin/aster-sysinfo "${SYSINFO_ARGS[@]}" &
    SYSINFO_PID=$!
    log_info "aster-sysinfo started with PID $SYSINFO_PID"
fi

# ------------------------------------------------------------------------------
# 4. Optional Custom Sensor Script Hook
# ------------------------------------------------------------------------------
if [ -x "/config/custom_sensors.sh" ]; then
    log_info "Found custom sensor script /config/custom_sensors.sh. Starting..."
    /config/custom_sensors.sh "$SENSOR_FILE" &
    CUSTOM_PID=$!
fi

# ------------------------------------------------------------------------------
# 5. Determine Paths & Start Display Controller (asterctl)
# ------------------------------------------------------------------------------
CONFIG_FILE="${PANEL_CONFIG:-}"
if [ -z "$CONFIG_FILE" ]; then
    if [ -f /config/monitor.json ]; then
        CONFIG_FILE="/config/monitor.json"
    else
        CONFIG_FILE="/app/default-cfg/monitor.json"
    fi
fi

MAPPING_FILE="/config/sensor-mapping/sysinfo-to-aoostar.cfg"
if [ ! -f "$MAPPING_FILE" ]; then
    MAPPING_FILE="/app/default-cfg/sensor-mapping/sysinfo-to-aoostar.cfg"
fi

ASTERCTL_ARGS=(
    "--config" "$CONFIG_FILE"
    "--config-dir" "/config"
    "--font-dir" "/app/fonts"
    "--sensor-path" "/tmp/sensors"
    "--sensor-mapping" "$MAPPING_FILE"
)

if [ -f "$SETTINGS_FILE" ]; then
    ASTERCTL_ARGS+=("--settings" "$SETTINGS_FILE")
fi

if [ -e "$TARGET_DEVICE" ]; then
    ASTERCTL_ARGS+=("--device" "$TARGET_DEVICE")
elif [ -n "${USB_ID:-}" ]; then
    ASTERCTL_ARGS+=("--usb" "$USB_ID")
fi

log_info "Launching asterctl with arguments: ${ASTERCTL_ARGS[*]}"

# Supervisor loop to keep running and recover from transient disconnects
while true; do
    log_info "Starting asterctl display daemon..."
    /usr/local/bin/asterctl "${ASTERCTL_ARGS[@]}" &
    ASTERCTL_PID=$!

    # Wait for asterctl process
    wait "$ASTERCTL_PID" || true
    EXIT_CODE=$?

    log_warn "asterctl process exited with code $EXIT_CODE."
    
    # Sleep 3 seconds before attempting restart
    sleep 3
done
