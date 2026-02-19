# macOS XR50 Status (Archived Investigation)

This document tracks the final macOS findings for XR50 in this repo.

## Final Outcome

macOS support is **diagnostic only** right now.

What works:
- Device enumeration and info queries
- Continuous SLAM packet ingestion (`~880-970 Hz` observed)
- Coexistence with Ultraleap is better with `hidapi` than `rusb`

What does not work:
- Usable pose output (translation remains `[0,0,0]`)
- Confidence remains around `0.001`
- Rotation is either frozen or unstable depending on parse path

Decision:
- Use **Linux or Windows** for XR50 runtime.
- Keep macOS only for frontend/dev and protocol diagnostics.

## Backend Findings

### `rusb` backend (libusb, usually with sudo)

Observed repeatedly:
- `No such device`, `Pipe error`, `device disconnected`
- Re-enumeration windows (`XR50 not found` retries)
- Can reset/disrupt devices on shared hub (XR50 + Ultraleap)

Use only for low-level debugging.

### `hidapi` backend (no sudo)

Observed repeatedly:
- Edge stream starts successfully
- SLAM packets stream stably
- Does not provide real tracking (still `conf ~0.001`, zero translation)

Use this path if you need macOS packet capture without disrupting the USB stack.

## Best-Known Diagnostic Command (macOS)

```bash
XVISIO_MAC_BACKEND=hidapi \
XVISIO_UVC_MODE=1 \
XVISIO_ROTATION_ENABLED=1 \
XVISIO_ENABLE_STEREO_INIT=1 \
XVISIO_REOPEN_AFTER_CONFIG=0 \
XVISIO_ROTATION_PARSE=quat \
./run-macos.sh stream
```

Expected output characteristics:
- SLAM packet headers like `01 a2 33`
- High sample rate
- non-usable pose (`pos=[0,0,0]`, `conf=0.001`)

## Notes for Future macOS Retry

Keep these artifacts:
- `xvisio-rs/examples/macos_diag.rs`
- `xvisio-rs/run-macos.sh`
- `xvisio-rs/src/device.rs` macOS backend switching/env toggles
- `xvisio-rs/src/slam.rs` raw packet logging + parse switches

If retrying later, start by validating two things separately:
1. Command sequence parity with known-good Linux/Windows runtime behavior.
2. Packet schema/pose parse parity against captured Linux/Windows raw packets.

