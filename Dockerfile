# syntax=docker/dockerfile:1

# ------------------------------------------------------------------------------
# Stage 1: Build Stage (Rust on Alpine Linux)
# ------------------------------------------------------------------------------
FROM rust:alpine3.21 AS builder

# Install build dependencies required by serialport / libudev and Rust crates
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    eudev-dev \
    build-base \
    linux-headers

WORKDIR /build

# Copy entire Rust workspace from aoostar-rs
COPY aoostar-rs/ /build/

# Build release binaries for all workspace members (asterctl, aster-sysinfo, asterctl-lcd)
RUN cargo build --release

# ------------------------------------------------------------------------------
# Stage 2: Runtime Stage (Minimal Alpine Linux)
# ------------------------------------------------------------------------------
FROM alpine:3.21

# Install runtime dependencies:
# - eudev-libs: for USB device discovery via libudev
# - smartmontools: for disk drive temperature retrieval via smartctl
# - tzdata: for local timezone support (ensures clock displays correctly)
# - bash, coreutils, procps: for reliable entrypoint script execution & sys stats
RUN apk add --no-cache \
    eudev-libs \
    smartmontools \
    tzdata \
    bash \
    coreutils \
    procps \
    findutils \
    util-linux

# Set up application directories
WORKDIR /app
RUN mkdir -p /app/fonts /app/default-cfg /config /tmp/sensors

# Copy compiled binaries from builder stage
COPY --from=builder /build/target/release/asterctl /usr/local/bin/asterctl
COPY --from=builder /build/target/release/aster-sysinfo /usr/local/bin/aster-sysinfo

# Copy fonts and default configuration templates
COPY aoostar-rs/fonts/ /app/fonts/
COPY aoostar-rs/cfg/ /app/default-cfg/

# Copy entrypoint script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh /usr/local/bin/asterctl /usr/local/bin/aster-sysinfo

# Default Environment Variables
ENV DEVICE="/dev/ttyACM0" \
    USB_ID="0416:90A1" \
    TZ="UTC" \
    PANEL_CONFIG="" \
    SENSOR_REFRESH="2" \
    DISK_REFRESH="10" \
    ENABLE_SMART="false" \
    ENABLE_SYSINFO="true" \
    AUTO_OFF_ON_STOP="true" \
    RUST_LOG="info"

# Persistent configuration volume for custom panels and mappings
VOLUME ["/config"]

ENTRYPOINT ["/entrypoint.sh"]
