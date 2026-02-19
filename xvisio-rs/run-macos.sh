#!/bin/bash
# Run xvisio examples with sudo on macOS.
#
# macOS requires root privileges for SLAM streaming because libusb needs
# to detach the kernel HID driver from the XR50's USB interface.
#
# Device discovery and info queries work without sudo.
#
# Usage:
#   ./run-macos.sh stream          # SLAM streaming
#   ./run-macos.sh stream_json     # SLAM streaming with JSON output
#   ./run-macos.sh server          # WebSocket SLAM server
#   ./run-macos.sh macos_diag      # Low-level diagnostic
#   ./run-macos.sh enumerate       # List devices (no sudo needed)
#   ./run-macos.sh info            # Device info (no sudo needed)

set -e

EXAMPLE="${1:-stream}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Build first (doesn't need sudo)
echo "Building example: $EXAMPLE"
cargo build --release --example "$EXAMPLE" --manifest-path "$SCRIPT_DIR/Cargo.toml"

BINARY="$SCRIPT_DIR/target/release/examples/$EXAMPLE"
MAC_BACKEND="$(printf '%s' "${XVISIO_MAC_BACKEND:-rusb}" | tr '[:upper:]' '[:lower:]')"

# Examples that don't need sudo
case "$EXAMPLE" in
    enumerate|info)
        echo ""
        echo "Running: $BINARY"
        RUST_LOG=info "$BINARY"
        exit 0
        ;;
esac

# SLAM examples:
# - rusb backend needs sudo (kernel driver detach/claim)
# - hidapi backend runs as normal user (shared HID open)
echo ""
echo "Effective macOS backend: $MAC_BACKEND"
if [ "$MAC_BACKEND" = "hidapi" ]; then
    echo "Using macOS HIDAPI backend (no sudo)."
    echo "Running: $BINARY"
    echo ""
    RUST_LOG=info "$BINARY"
elif [ "$MAC_BACKEND" = "rusb" ]; then
    echo "Using macOS RUSB backend (requires root for detach/claim)."
    echo "Running: sudo $BINARY"
    echo ""
    sudo --preserve-env=RUST_LOG,XVISIO_MAC_BACKEND,XVISIO_UVC_MODE,XVISIO_ROTATION_ENABLED,XVISIO_ROTATION_PARSE,XVISIO_CLAIM_ALL_INTERFACES,XVISIO_PRECONDITION_CYCLES,XVISIO_ENABLE_STEREO_INIT,XVISIO_REOPEN_AFTER_CONFIG,XVISIO_REOPEN_AFTER_EDGE_START,XVISIO_ALLOW_DETACH_FALLBACK,XVISIO_DEBUG_RAW \
      RUST_LOG=info "$BINARY"
else
    echo "Unknown XVISIO_MAC_BACKEND='$MAC_BACKEND' (expected: hidapi|rusb)"
    exit 2
fi
