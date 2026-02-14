# xvisio-rs — Cross-Platform Rust SDK for XVisio XR50

Cross-platform Rust SDK for the XVisio XR50 6DOF tracking sensor.

- **Windows/Linux**: uses hidapi for both commands and SLAM streaming
- **macOS**: uses hidapi for device discovery/queries + rusb (libusb) for SLAM streaming

Includes an all-in-one Rust server that replaces Node.js `server.js` for the visual-test frontend.

## Architecture

```
XR50 sensor (USB HID)
    | hidapi (Windows/Linux) or rusb/libusb (macOS) — 950 Hz raw SLAM
xvisio-rs Rust library
    | crossbeam-channel
server.rs example binary
    |-- HTTP: serves visual-test/dist/ static files
    +-- WebSocket: broadcasts 6DOF pose JSON at ~60 Hz
         |
Browser (React + Three.js) at http://localhost:8080
```

## File Structure

```
xvisio-rs/
  Cargo.toml          # hidapi, rusb, thiserror, crossbeam-channel, log, bitflags
  build.rs            # cbindgen -> include/xvisio.h (C header for FFI)
  src/
    lib.rs            # Public API: Device, SlamStream, SlamSample, Pose, etc.
    device.rs         # Device enumeration + open via hidapi (VID=0x040E, PID=0xF408)
    hid.rs            # HID transport: write/get_input_report for commands
    slam.rs           # SLAM reader thread: hidapi (Win/Linux) or rusb (macOS) -> channel
    protocol.rs       # USB protocol: build_command, parse_slam_packet, quaternion_to_euler
    types.rs          # Pose, SlamSample, Features, SlamMode
    error.rs          # XvisioError enum
    ffi.rs            # C FFI exports (xv_open, xv_slam_start, etc.)
  examples/
    enumerate.rs      # List connected XR50 devices
    info.rs           # Print UUID, version, features
    stream.rs         # Stream raw pose data to console
    stream_json.rs    # Stream JSON lines to stdout (for piping)
    server.rs         # All-in-one HTTP + WebSocket + SLAM server
```

## Prerequisites

All platforms need:
- **Rust** via rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** (only to build visual-test/dist/ once): install via nvm or package manager
- **XR50 connected via USB**

### Linux (Ubuntu / WSL2)

```bash
sudo apt install libhidapi-dev libudev-dev pkg-config build-essential
```

Udev rule for non-root access (create then reload):

```bash
echo 'SUBSYSTEM=="hidraw", ATTRS{idVendor}=="040e", ATTRS{idProduct}=="f408", MODE="0666"' \
  | sudo tee /etc/udev/rules.d/99-xr50.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

**Critical: Unbind uvcvideo driver.** Linux's `uvcvideo` kernel driver binds to the
XR50's UVC camera interfaces and prevents the on-device edge SLAM from accessing its
stereo cameras (SLAM outputs identity pose with zero confidence). You must unload it:

```bash
sudo rmmod uvcvideo
```

To make this permanent, blacklist uvcvideo for the XR50 (won't affect other webcams
if they're plugged in before the XR50):

```bash
echo 'blacklist uvcvideo' | sudo tee /etc/modprobe.d/xr50-no-uvc.conf
```

WSL2 needs USB passthrough via usbipd (run from Windows PowerShell as admin):

```powershell
# Install: winget install usbipd
usbipd list                                    # find XR50 bus ID
usbipd bind --busid <BUS_ID>                   # one-time bind
usbipd attach --wsl --busid <BUS_ID>           # attach to WSL2
```

Then verify in WSL2: `lsusb | grep 040e`

**WSL2 troubleshooting:**
- Device can change bus ID when replugged (e.g. `3-1` → `4-1` → `7-1`) — re-run `usbipd list` to find new ID
- "Port Reset Failed" errors: unplug XR50 for 10+ seconds, replug to a different USB port
- After `usbipd attach`, always run `sudo rmmod uvcvideo` before using the XR50

### macOS (Apple Silicon / M1)

No extra packages needed — hidapi links against IOKit, and rusb uses a vendored libusb.

```bash
# Rust should default to aarch64-apple-darwin on M1
rustup show  # verify target
```

**SLAM streaming requires `sudo`** on macOS. The XR50 does USB re-enumeration when
switching to SLAM mode, which invalidates macOS IOKit HID handles. The SDK uses
rusb (libusb) for SLAM on macOS, which needs root to detach the kernel HID driver.

Device discovery and info queries (enumerate, info) work without sudo.

```bash
# No sudo needed
cargo run --release --example enumerate
cargo run --release --example info

# Sudo required for SLAM streaming
sudo cargo run --release --example stream
sudo cargo run --release --example server
```

Or use the helper script: `./run-macos.sh stream`

### Windows

No special drivers needed — hidapi uses the Windows HID driver directly.
Install Rust from https://rustup.rs (select MSVC toolchain).

## Build & Run

```bash
# From the vibestar/ root directory

# 1. Build the visual-test frontend (one-time, needs Node.js)
cd visual-test
npm install
npm run build      # creates visual-test/dist/
cd ..

# 2. Build and run the server
cd xvisio-rs
cargo run --release --example server       # Windows/Linux
# sudo cargo run --release --example server  # macOS

# 3. Open browser to http://localhost:8080
```

The server will:
- Open the XR50 and stream SLAM at ~950 Hz
- Serve the visual-test frontend on HTTP port 8080
- Broadcast 6DOF pose JSON over WebSocket at ~60 Hz

## Other Examples

```bash
cargo run --example enumerate      # List XR50 devices
cargo run --example info           # UUID, version, features
cargo run --example stream         # Raw pose to console at ~950 Hz
cargo run --example stream_json    # JSON lines to stdout at ~950 Hz
```

## WebSocket JSON Format

```json
{"x":0.0210,"y":0.0020,"z":0.0280,"roll":5.2,"pitch":3.1,"yaw":1.4,"t":1596314}
```

| Field | Description |
|-------|-------------|
| x, y, z | Translation in meters |
| roll, pitch, yaw | Euler angles in degrees (YXZ order, Z-flipped for Three.js) |
| t | Device timestamp in microseconds |

## Test Results

### Windows (XR50 connected, native USB)

| Example | Result |
|---------|--------|
| enumerate | Lists XR50: UUID, version, features |
| stream | 949-994 Hz 6DOF pose streaming |
| stream_json | JSON lines at ~950 Hz |
| server | Full pipeline: 950 samples/s, 60 ws/s to browser, stable |

### Linux / WSL2 (XR50 via usbipd USB passthrough)

| Example | Result |
|---------|--------|
| enumerate | Lists XR50: UUID, version, features |
| stream | ~949 Hz 6DOF pose streaming (after uvcvideo unloaded) |
| server | Full pipeline: 948 samples/s, 59 ws/s to browser, stable |

**Note:** Without `sudo rmmod uvcvideo`, SLAM outputs identity pose with zero confidence.
See the Linux prerequisites section above for the fix.

### macOS (Apple Silicon M1 Max)

| Example | Result |
|---------|--------|
| enumerate | Lists XR50: UUID, version, features (no sudo) |
| info | UUID, version, features (no sudo) |
| stream | Requires sudo — pending test |
| server | Requires sudo — pending test |

**macOS architecture**: Device discovery and HID commands use hidapi (IOKit).
SLAM streaming uses rusb (libusb) because the XR50 does USB re-enumeration when
switching to edge SLAM mode, which invalidates all IOKit HID handles. rusb needs
root to detach the macOS kernel HID driver (`AppleUSBHostHIDDevice`) from the
XR50's HID interface before claiming it for interrupt reads.

## Key Technical Details

**hidapi report ID**: XR50 uses 0x02 prefix for host-to-device and 0x01 for device-to-host.
hidapi `write()` uses byte[0] as the HID report ID. Since 0x02 is both the protocol prefix
and the report ID, `build_command()` output (63 bytes starting with 0x02) goes directly to
`write()`. Do NOT wrap it in an extra byte.

**Server threading**: SLAM thread reads at ~950 Hz but only broadcasts to WebSocket at ~60 Hz
(browser refresh rate). The SLAM thread is the sole writer to each WebSocket — the per-client
handler thread only monitors for disconnect. This eliminates mutex contention.

**HTTP chunked writes**: Large files (1MB+ JS bundle) are written in 64KB chunks to prevent
TCP send buffer overflow on Windows.
