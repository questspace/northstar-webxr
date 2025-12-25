# Project NorthStar macOS Setup Guide

This guide covers setting up the Project NorthStar AR headset on macOS with XVisio XR50 6DOF tracking and Ultraleap hand tracking.

## Hardware

| Component | Model | Status |
|-----------|-------|--------|
| Display | 2880×1600 @ 89Hz | ✅ Detected |
| 6DOF Tracking | XVisio SeerSense XR50 | ✅ Working (982 Hz) |
| Hand Tracking | Ultraleap Stereo IR 170 | ⏳ Pending |
| Integrator | PNS Hub 1.4 / Junction 1.0 | ✅ Connected |

## XVisio XR50 Setup

### Device Info

- **Serial:** XR501G10002222006282
- **Firmware:** V1.09 (2022-12-07)
- **USB VID/PID:** 0x040E / 0xF408

### Building libxvisio

```bash
cd libxvisio
mkdir build && cd build
cmake ..
make
```

### Running 6DOF Tracking

Requires root privileges for USB access:

```bash
sudo ./xvisio_test
```

**Output:**
```
XVisio SeerSense XR50 - 6DOF Tracking
=====================================

Device: XR501G10002222006282
Firmware: 1V1.04P31||xr50|V1.09|20221207_01|develop|56a1f2a

Features:
  Edge SLAM:  Yes
  Mixed SLAM: Yes
  Stereo:     Yes

Starting 6DOF tracking... (Ctrl+C to stop)

Pos: ( -0.12,   0.05,   0.03) m | Roll:  125.5° | Pitch:  -37.3° | Yaw:    1.8°
```

### Performance

| Metric | Value |
|--------|-------|
| Update Rate | ~982 Hz |
| Position | 3DOF (X, Y, Z in meters) |
| Rotation | 3DOF (Roll, Pitch, Yaw) |
| Latency | <1ms |

## Ultraleap Setup

### Installation

Download and install [Ultraleap Gemini](https://developer.leapmotion.com/tracking-software-download) for macOS.

### Verify Service

```bash
ps aux | grep -i leap
```

Should show `Ultraleap Hand Tracking.app` and `libtrack_server` running.

### Stereo IR 170 Connection

If device not detected:
1. Check ribbon cable connection to integrator board
2. Press and hold ● button on PNS Junction for 4 seconds
3. Wait for USB re-enumeration

## Display Configuration

The NorthStar display appears as a secondary monitor at 2880×1600 @ 89Hz.

**System Settings → Displays:**
- Arrange below primary display
- No scaling required

## Project Structure

```
vibestar/
├── libxvisio/           # XVisio XR50 driver
│   ├── src/             # Library source
│   ├── include/         # Headers
│   └── example/         # Test application
├── project-esky-unity/  # Unity integration
└── MACOS_SETUP_GUIDE.md # This file
```

## Next Steps

1. ✅ XVisio XR50 6DOF tracking - **Complete**
2. ⏳ Ultraleap hand tracking integration
3. ⏳ Unity project configuration
4. ⏳ Display calibration

## Resources

- [Project NorthStar Docs](https://docs.projectnorthstar.org)
- [XVisio Technology](https://www.xvisiotech.com)
- [Ultraleap Developer](https://developer.leapmotion.com)
