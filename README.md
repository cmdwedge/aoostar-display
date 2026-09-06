# Aoostar WTR Max Display Daemon (Docker & Unraid)

A high-performance, lightweight Rust container for driving the front LCD screen (`960x376`) on the **AOOSTAR WTR Max NAS** and **GEM12+ PRO**.

This replaces the heavy, insecure vendor Python application with an optimized, memory-safe Rust daemon built on Alpine Linux.

---

## Features

- **Lightweight & Fast**: Compiled multi-stage Alpine Rust build.
- **Real-Time Panels**: CPU load, RAM usage, storage stats, network bandwidth, and motherboard/component temperatures.
- **Differential Frame Caching**: Only sends modified pixel blocks over UART, saving CPU cycles and USB bus bandwidth.
- **Graceful Lifecycle Management**: Automatically powers off the LCD backlight when the container is stopped or restarted (`SIGTERM`/`SIGINT` handling).
- **Persistent Customization**: Automatically seeds configuration and templates into `/config` (`/mnt/user/appdata/aoostar-display` on Unraid) on first run.
- **Unraid 7.x Ready**: Includes an Unraid Community Apps XML template.

---

## Directory Structure

```
.
├── Dockerfile                        # Multi-stage Alpine Docker build
├── entrypoint.sh                     # Container lifecycle and supervision daemon
├── .dockerignore                     # Build context exclusions
├── config/                           # Default template configs & images
│   ├── monitor.json
│   ├── sensor-mapping/
│   │   ├── sysinfo-to-aoostar.cfg
│   │   └── sysinfo-to-aoostar-filter.cfg
│   ├── default_1_index.jpg
│   ├── default_1_hdd.jpg
│   ├── progress1.png
│   └── progress2.png
├── unraid-template/
│   └── aoostar-display.xml           # Unraid 7.x Community Apps template
├── aoostar-rs/                       # Upstream reverse-engineered Rust codebase
└── aoostar-wtr-max-truenas-display/  # TrueNAS Go reference codebase
```

---

## Building the Docker Image

To build the image locally on any Docker-capable machine:

```bash
docker build -t aoostar-display:latest .
```

---

## Running with Docker CLI

### Recommended (Host Network Mode):
Running with `--net=host` gives the container direct access to physical 10GbE network throughput and the server's real LAN IP address, without needing virtual bridge translation. Port `8744` is used for the web interface, which is free of conflicts with popular homelab containers.

```bash
docker run -d \
  --name aoostar-display \
  --net=host \
  --device=/dev/ttyACM0:/dev/ttyACM0 \
  -v /mnt/user/appdata/aoostar-display:/config \
  -v /var/local/emhttp:/var/local/emhttp:ro \
  -v /etc/localtime:/etc/localtime:ro \
  --restart unless-stopped \
  aoostar-display:latest
```

### Alternative (Bridge Network Mode):
If you must run in bridge mode, map port `8744` and mount `/proc/net/dev`:
```bash
docker run -d \
  --name aoostar-display \
  --net=bridge \
  -p 8744:8744 \
  --device=/dev/ttyACM0:/dev/ttyACM0 \
  -v /mnt/user/appdata/aoostar-display:/config \
  -v /var/local/emhttp:/var/local/emhttp:ro \
  -v /proc/net/dev:/host/proc/net/dev:ro \
  -v /etc/localtime:/etc/localtime:ro \
  --restart unless-stopped \
  aoostar-display:latest
```
> **Note on Bridge Mode**: In bridge mode, Docker isolates the container network stack. Throughput is read from `/host/proc/net/dev`, but the server's local LAN IP cannot be auto-detected directly by the Linux networking stack inside a bridge namespace.

---

## Screen Power Management

Display backlight control and sleep modes are configured in `/config/settings.json` (and automatically reloaded live without restarting the container):

```json
{
  "general": {
    "webPort": 8744,
    "switchTime": 30,
    "refreshInterval": 1,
    "tempUnit": "C",
    "activePanels": [1, 2, 3]
  },
  "network": {
    "interface": "auto"
  },
  "power": {
    "sleepScheduleEnabled": false,
    "sleepStartTime": "23:00",
    "sleepEndTime": "07:00",
    "sleepOnAllDisksStandby": false,
    "standbyDelaySeconds": 300,
    "wakeOnDiskActivity": true,
    "wakeOnNetworkActivity": false,
    "networkWakeThresholdBytes": 5242880
  }
}
```

### Power Management Features:
- **Night Mode / Sleep Schedule**: Blanks display backlight between `sleepStartTime` and `sleepEndTime` (handles overnight hours across midnight).
- **Drive Spindown Sleep**: Automatically blanks display when all array HDDs are in `Standby` for `standbyDelaySeconds` (e.g. 5 minutes).
- **Activity Wake**: Automatically wakes display when any array drive spins up or when network transfer exceeds `networkWakeThresholdBytes`.
- **Manual Power Override**: Echo `off` or `on` to `/tmp/screen_power` (e.g. `docker exec aoostar-display sh -c "echo off > /tmp/screen_power"`) to manually control the screen via Home Assistant or scripts.

---

## Unraid 7.x Installation

1. **Copy the XML Template**:
   Place [aoostar-display.xml](unraid-template/aoostar-display.xml) into `/boot/config/plugins/dockerMan/templates-user/` on your Unraid flash drive.
2. **Add Container in Unraid WebUI**:
   - Go to the **Docker** tab in Unraid.
   - Click **Add Container** and select the `aoostar-display` template from the template dropdown.
   - Verify the serial device `/dev/ttyACM0` matches your hardware (`ls -l /dev/ttyACM*` in Unraid Terminal).
   - Set your **Timezone** (e.g. `America/Chicago`, `Europe/London`, `Australia/Sydney`) so the screen clock matches local time.
   - Click **Apply**.
3. **Customize Panels & Settings**:
   Your `/mnt/user/appdata/aoostar-display/` directory will automatically contain `settings.json`, `monitor.json`, background pictures, and sensor mappings for quick adjustments.

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DEVICE` | `/dev/ttyACM0` | Path to USB UART device. Auto-detects if omitted. |
| `USB_ID` | `0416:90A1` | USB VID:PID fallback identifier. |
| `TZ` | `UTC` | Timezone for clock display (auto-detects from host if UTC). |
| `SWITCH_TIME` | `30` | Seconds each panel displays before rotating. |
| `NETWORK_INTERFACE` | `auto` | Primary network interface override (e.g. `br0`, `eth1`). |
| `TEMP_UNIT` | `C` | Temperature unit: `C` for Celsius, `F` for Fahrenheit. |
| `SLEEP_SCHEDULE_ENABLED` | `false` | Enable automatic display blanking during sleep hours. |
| `SLEEP_START_TIME` | `23:00` | Start of sleep schedule (24h HH:MM). |
| `SLEEP_END_TIME` | `07:00` | End of sleep schedule (24h HH:MM). |
| `SLEEP_ON_STANDBY` | `false` | Turn off display when all array drives are in Standby. |
| `STANDBY_DELAY` | `300` | Seconds to wait after all drives standby before sleeping. |
| `SENSOR_REFRESH` | `2` | Interval in seconds between system metric updates. |
| `DISK_REFRESH` | `10` | Interval in seconds between disk metric updates. |
| `ENABLE_SMART` | `false` | Enable SMART drive temperature polling via `smartctl`. |
| `AUTO_OFF_ON_STOP` | `true` | Turn off LCD backlight on container stop. |
| `RUST_LOG` | `info` | Logging verbosity (`info`, `debug`, `warn`, `error`). |
