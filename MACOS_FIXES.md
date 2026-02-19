# libxvisio macOS Fixes — Issues Identified via Windows Testing

Based on comparing the official Windows SDK behavior (xslam_sdk.dll) and raw HID protocol
testing with the libxvisio open-source driver code.

## Critical Issues

### 1. SLAM Event Loop Too Slow (100ms sleep = ~10 Hz processing)

**File:** `src/slam.cpp:60-63`

```cpp
while (runThread) {
    libusb_handle_events(context);
    std::this_thread::sleep_for(std::chrono::milliseconds(100));  // <-- Problem
}
```

**Problem:** The device streams SLAM data at **~950 Hz** (confirmed via Windows testing).
The 100ms sleep between `libusb_handle_events` calls means the library only processes
events ~10 times per second. While the async callback does fire when data arrives, the
completion handler only runs when `libusb_handle_events` is invoked. This creates:

- **Batching:** ~95 packets arrive in bursts every 100ms instead of streaming smoothly
- **Latency:** Up to 100ms of added latency on pose data
- **Buffer overflow risk:** libusb internal buffers may overflow if not drained fast enough

**Fix:** Remove or drastically reduce the sleep:

```cpp
while (runThread) {
    struct timeval tv = { 0, 1000 }; // 1ms timeout
    libusb_handle_events_timeout(context, &tv);
}
```

Or use `libusb_handle_events_timeout` with a 1ms timeout to balance CPU usage and responsiveness.

### 2. Single Transfer In Flight — Packet Loss at 950 Hz

**File:** `src/slam.cpp:48-53`

```cpp
libusb_transfer* transfer = libusb_alloc_transfer(0);
const auto buffer = std::make_shared<std::array<uint8_t, 64>>();
libusb_fill_interrupt_transfer(transfer, handle, 0x83, buffer->data(), 63, ...);
libusb_submit_transfer(transfer);
```

**Problem:** Only one transfer is ever submitted. At 950 Hz, each packet arrives every ~1.05ms.
If the callback processing + resubmission takes longer than this, a packet is lost. The
official SDK likely uses multiple in-flight transfers.

**Fix:** Use a ring of 4-8 pre-allocated transfers:

```cpp
constexpr int NUM_TRANSFERS = 8;
for (int i = 0; i < NUM_TRANSFERS; i++) {
    auto* transfer = libusb_alloc_transfer(0);
    auto* buffer = new uint8_t[64];
    libusb_fill_interrupt_transfer(transfer, handle, 0x83, buffer, 63,
                                    &Slam::usbCallback, &callbacks, 5000);
    libusb_submit_transfer(transfer);
}
```

### 3. Quaternion Order Mismatch vs Official SDK

**File:** `src/types/pose.cpp:9-43`

**Problem:** libxvisio's `matrixToQuaternion` returns quaternion as **[w, x, y, z]**:
```cpp
quaternion[0] = 0.25 * scale;                           // w
quaternion[1] = (matrix[2][1] - matrix[1][2]) / scale;  // x
quaternion[2] = (matrix[0][2] - matrix[2][0]) / scale;  // y
quaternion[3] = (matrix[1][0] - matrix[0][1]) / scale;  // z
```

The official XVisio SDK (`xv-types.h`) uses **[qx, qy, qz, qw]** order:
```cpp
// From xv-types.h:
// "Get the quaternion [qx,qy,qz,qw] of the rotation."
```

Any application using libxvisio's quaternion output and expecting the official SDK's
convention will have rotation errors.

**Fix:** Reorder the quaternion output to match the official SDK:
```cpp
quaternion[0] = (matrix[2][1] - matrix[1][2]) / scale;  // qx
quaternion[1] = (matrix[0][2] - matrix[2][0]) / scale;  // qy
quaternion[2] = (matrix[1][0] - matrix[0][1]) / scale;  // qz
quaternion[3] = 0.25 * scale;                           // qw
```

Or document the different convention clearly so consumers know which order to expect.

## Moderate Issues

### 4. macOS Kernel Driver Seizure

**File:** `src/device/device.cpp:29-36`

```cpp
#ifdef __linux__
    if (libusb_kernel_driver_active(handle, 3) == 1) {
        libusb_detach_kernel_driver(handle, 3);
    }
#endif
```

**Problem:** On macOS, the IOKit HID driver (`AppleUserHIDDevice`) will also claim HID
interface 3 by default. The `#ifdef __linux__` guard means macOS never detaches it.
libusb on macOS supports `libusb_detach_kernel_driver`, so the guard should include macOS.

**Fix:**
```cpp
#if !defined(_WIN32)
    // Linux and macOS both need kernel driver detachment
    if (libusb_kernel_driver_active(handle, 3) == 1) {
        if (int res = libusb_detach_kernel_driver(handle, 3); res != 0) {
            std::cerr << "Unable to detach kernel driver" << std::endl;
            throw std::runtime_error(libusb_strerror(res));
        }
    }
#endif
```

Or use `libusb_set_auto_detach_kernel_driver(handle, 1)` which works on both Linux and macOS
and automatically reattaches the driver when the interface is released.

### 5. USB Device List Leak

**File:** `src/xvisio.cpp:15-23`

```cpp
libusb_device **list = nullptr;
const ssize_t nDevices = libusb_get_device_list(usb_ctx, &list);
for (int i = 0; i < nDevices; ++i) { ... }
// Missing: libusb_free_device_list(list, 1);
```

**Problem:** `libusb_get_device_list` allocates a list that must be freed with
`libusb_free_device_list`. The current code leaks this list. While not a
functional issue, it leaks memory and reference counts.

**Fix:** Add `libusb_free_device_list(list, 1);` after the enumeration loop.

### 6. Extended SLAM Packet Data Ignored

**File:** `src/slam.cpp:72-88`

**Problem:** The SLAM callback only parses bytes [3..36] of the 63-byte packet
(timestamp, translation, rotation). Bytes [37..62] contain additional data that
is likely:

- **[37..42]:** Accelerometer data (3 × int16, same scale factor)
- **[43..48]:** Gyroscope data (3 × int16)
- **[49..56]:** Additional sensor data (velocity estimates?)
- **[57..58]:** Confidence/status indicator

The `Device` class even has an unused `imuBias` field, suggesting IMU support was planned.
The official SDK's `Pose` type includes `linearVelocity`, `angularVelocity`,
`linearAcceleration`, `angularAcceleration`, and `confidence` fields that likely
come from this extended data.

**Fix:** Parse and expose the extended packet fields. At minimum, expose the raw
bytes so consumers can experiment.

### 7. No Error Handling on Transfer Resubmission

**File:** `src/slam.cpp:68-69`

```cpp
if (transfer->status == LIBUSB_TRANSFER_COMPLETED) {
    libusb_submit_transfer(transfer);
    // No error check!
```

**Problem:** If `libusb_submit_transfer` fails (e.g., device disconnected), the
error is silently ignored and the SLAM stream stops without notification.

**Fix:** Check the return value and signal the error to the event loop:
```cpp
if (transfer->status == LIBUSB_TRANSFER_COMPLETED) {
    if (libusb_submit_transfer(transfer) != 0) {
        // Signal error, stop the stream
        return;
    }
```

## Minor Issues

### 8. Hotplug Callback Cast

**File:** `src/xvisio.cpp:30`

```cpp
reinterpret_cast<libusb_hotplug_callback_fn>(&XVisio::hotPlugCallback)
```

**Problem:** Casting a static member function pointer to `libusb_hotplug_callback_fn`
via `reinterpret_cast` is technically undefined behavior. The function signature should
match exactly: `int (*)(libusb_context*, libusb_device*, libusb_hotplug_event, void*)`.
The current signature uses `std::vector<...>*` instead of `void*` for the user_data parameter.

**Fix:** Use `void*` in the signature and cast inside:
```cpp
static int LIBUSB_CALL hotPlugCallback(libusb_context*, libusb_device*,
                                        libusb_hotplug_event, void* user_data);
```

### 9. XVisio(uint32_t timeout) Constructor Empty

**File:** `src/xvisio.cpp:44-46`

```cpp
XVisio::XVisio(uint32_t timeout) {
    // Empty!
}
```

This constructor does nothing — no USB init, no device enumeration. Using it will result
in a crash when `getDevices()` is called (null `usb_ctx`).

## Summary of Fixes Priority

| Priority | Issue | Impact |
|----------|-------|--------|
| P0 | Event loop 100ms sleep | 90%+ latency, possible data loss |
| P0 | Single transfer in flight | Packet loss at 950 Hz |
| P1 | macOS kernel driver seizure | Device may not open on macOS |
| P1 | Quaternion order [w,x,y,z] vs [x,y,z,w] | Incorrect rotations for SDK-compatible apps |
| P2 | Device list leak | Memory leak |
| P2 | Extended packet data ignored | Missing IMU/velocity/confidence data |
| P2 | Transfer resubmission error handling | Silent stream death |
| P3 | Hotplug callback UB | Technically UB, works in practice |
| P3 | Empty timeout constructor | Crash if used |
