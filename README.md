# Vibestar

macOS development environment for [Project NorthStar](https://docs.projectnorthstar.org) AR headset.

## Hardware Status

| Component | Model | Status |
|-----------|-------|--------|
| 6DOF Tracking | XVisio SeerSense XR50 | ✅ Working (982 Hz) |
| Hand Tracking | Ultraleap Stereo IR 170 | ✅ Working (87 FPS, 20 joints/hand) |
| Display | 2880×1600 @ 89Hz | ✅ Detected |

## Quick Start

### 6DOF Head Tracking (XVisio XR50)

```bash
# Build driver
cd libxvisio && mkdir -p build && cd build && cmake .. && make

# Run visual test
cd ../../visual-test && npm install
sudo ../libxvisio/build/xvisio_test | node server.js
```
Open http://localhost:8080 — move the XR50 to see real-time 6DOF tracking.

### Hand Tracking (Ultraleap SIR170)

1. Install [Ultraleap Hand Tracking](https://developer.leapmotion.com/tracking-software-download) (Gemini V5+)
2. The app automatically runs a WebSocket server on `ws://localhost:6437`

```bash
cd visual-test && python3 -m http.server 8080
```
Open http://localhost:8080/hands.html — wave your hands in front of the SIR170.

## Architecture

```
XR50 Sensor → xvisio_test (C++) → JSON stdout → Node.js → WebSocket → Three.js
SIR170 → Ultraleap Service → WebSocket (:6437) → Three.js
```

### Combined / NorthStar VR

Ensure Ultraleap Hand Tracking app is running, then:

```bash
cd visual-test
sudo ../libxvisio/build/xvisio_test | node server.js
```

| URL | Description |
|-----|-------------|
| http://localhost:8080/combined.html | Desktop preview (6DOF + hands) |
| http://localhost:8080/northstar.html | **NorthStar stereo VR** |

**NorthStar controls:**
- **F** — Fullscreen (drag window to NorthStar display first)
- **H** — Hide debug UI
- Renders 1440×1600 per eye @ 89Hz

## Structure

```
├── libxvisio/              # XVisio XR50 driver (C++)
├── ultraleap-websocket/    # Ultraleap WebSocket server
├── visual-test/            # Three.js visualization
│   ├── index.html          # 6DOF tracking test
│   ├── hands.html          # Hand tracking test
│   ├── combined.html       # Combined XR50 + Ultraleap
│   └── server.js           # XR50 WebSocket bridge
└── project-esky-unity/     # Unity integration (WIP)
```

## Requirements

- macOS
- [Ultraleap Hand Tracking](https://developer.leapmotion.com/tracking-software-download) (Gemini V5+)
- CMake, libusb, libwebsockets (`brew install cmake libusb libwebsockets`)
- Node.js 18+
