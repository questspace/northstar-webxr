# libxvisio

Open-source driver for the [XVisio SeerSense XR50](https://www.xvisiotech.com/product/seersense-xr50/) 6DOF tracking sensor.

## Features

- ✅ 6DOF pose tracking (position + rotation) at ~1000 Hz
- ✅ Device information (UUID, firmware version, capabilities)
- ✅ Edge mode SLAM (on-device processing)
- ✅ macOS and Linux support

## Requirements

- CMake 3.20+
- C++20 compiler
- libusb-1.0

## Building

```bash
mkdir build && cd build
cmake ..
make
```

## Usage

**Note:** Requires root privileges for USB access on macOS.

```bash
sudo ./xvisio_test
```

### Output

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

## API Example

```cpp
#include "xvisio.h"

void callback(const xv::Pose& pose) {
    // pose.position[0..2] - X, Y, Z in meters
    // pose.quaternion[0..3] - W, X, Y, Z rotation
    // pose.timestamp - microseconds
}

int main() {
    xv::XVisio xvisio;
    auto device = xvisio.getDevices()[0];
    
    auto slam = device->getSlam();
    slam->registerSlamCallback(&callback);
    slam->start(xv::Slam::mode::Edge);
    
    // ... run until done ...
    
    slam->stop();
}
```

## License

MIT
