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

# Examples that don't need sudo
case "$EXAMPLE" in
    enumerate|info)
        echo ""
        echo "Running: $BINARY"
        RUST_LOG=info "$BINARY"
        exit 0
        ;;
esac

# SLAM examples need sudo on macOS
echo ""
echo "SLAM streaming requires root on macOS (to detach kernel HID driver)."
echo "Running: sudo $BINARY"
echo ""
sudo RUST_LOG=info "$BINARY"
