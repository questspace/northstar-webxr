# XVisio XR50 USB/HID Protocol Documentation

Reverse-engineered from the official Windows SDK (`xslam_sdk.dll`, `xslam-drivers.dll`)
and the open-source `libxvisio` driver, validated with live hardware testing.

## Device Identification

| Property | Value |
|----------|-------|
| USB Vendor ID | `0x040E` |
| USB Product ID | `0xF408` |
| Device Name | XVisio XR50 |
| Firmware Version (tested) | `1V1.04P31\|\|xr50\|V1.09\|20221207_01\|develop\|56a1f2a.` |
| UUID (tested) | `XR501G10002222006282` |

## USB Interface Layout

The XR50 exposes 4 USB interfaces in a single configuration:

| Interface | Class | Description | Endpoints |
|-----------|-------|-------------|-----------|
| 0 | 255 (Vendor) | Bulk data (firmware, calibration) | EP 0x01 OUT (bulk), EP 0x81 IN (bulk) |
| 1 | 14 (Video) | UVC Control | None |
| 2 | 14 (Video) | UVC Streaming (stereo cameras) | EP 0x82 IN (bulk) |
| 3 | 3 (HID) | Control + SLAM data | EP 0x83 IN (interrupt, 64B, 1ms) |

**Interface 3** is the primary interface for:
- Sending HID control commands (via SET_REPORT/GET_REPORT control transfers)
- Receiving SLAM pose data (via interrupt transfers on EP 0x83)

## HID Control Protocol

### Transport Layer

Commands use HID SET_REPORT and GET_REPORT control transfers on interface 3:

**Sending a command (SET_REPORT):**
```
bmRequestType: 0x21 (OUT | CLASS | INTERFACE)
bRequest:      0x09 (SET_REPORT)
wValue:        0x0202 (Output Report, Report ID 2)
wIndex:        3 (Interface 3)
wLength:       63
Data:          [0x02, cmd_byte_0, cmd_byte_1, ...]
```

**Reading a response (GET_REPORT):**
```
bmRequestType: 0xA1 (IN | CLASS | INTERFACE)
bRequest:      0x01 (GET_REPORT)
wValue:        0x0101 (Input Report, Report ID 1)
wIndex:        3 (Interface 3)
wLength:       63
Data:          [0x01, cmd_echo_0, cmd_echo_1, ..., response_data...]
```

### Direction Prefixes

| Prefix | Direction | Description |
|--------|-----------|-------------|
| `0x02` | Host -> Device | Command sent to device |
| `0x01` | Device -> Host | Response from device |

The response echoes the command bytes after the `0x01` prefix, followed by the response data.
Response data starts at offset `1 + command_length`.

### Command Reference

#### Read UUID

Returns the device's unique serial number string (null-terminated).

```
Command:  [0x02, 0xFD, 0x66, 0x00, 0x02]  (4 cmd bytes)
Response: [0x01, 0xFD, 0x66, 0x00, 0x02, UUID_string...]
```

Example response: `"XR501G10002222006282"` (20 chars + null)

#### Read Firmware Version

Returns the firmware version string (null-terminated).

```
Command:  [0x02, 0x1C, 0x99]  (2 cmd bytes)
Response: [0x01, 0x1C, 0x99, version_string...]
```

Example response: `"1V1.04P31||xr50|V1.09|20221207_01|develop|56a1f2a."` (50 chars)

#### Read Features Bitmap

Returns a 32-bit bitmap of supported features.

```
Command:  [0x02, 0xDE, 0x62, 0x01]  (3 cmd bytes)
Response: [0x01, 0xDE, 0x62, 0x01, feat_0, feat_1, feat_2, feat_3, ...]
```

Features bitmap (little-endian uint32 at response offset 4):

| Bit | Feature |
|-----|---------|
| 0 | Edge SLAM mode |
| 1 | Mixed SLAM mode |
| 2 | Stereo cameras |
| 3 | RGB camera |
| 4 | ToF camera |
| 5 | IA (Intelligent Assistant) |
| 6 | SGBM (Semi-Global Block Matching) |
| 10 | Eye tracking |
| 12 | Face ID |

Tested device returns `0x00000007` = Edge + Mixed + Stereo.

#### Configure Device Mode

Sets the device's operating mode for SLAM processing.

```
Command:  [0x02, 0x19, 0x95, edge6dof, uvcMode, embeddedAlgo]
Response: [0x01, 0x19, 0x95, ...]
```

| Parameter | Values | Description |
|-----------|--------|-------------|
| `edge6dof` | 0 or 1 | 1 = Edge SLAM (on-device), 0 = Host SLAM |
| `uvcMode` | 0 or 1 | UVC camera streaming mode |
| `embeddedAlgo` | 0 or 1 | 1 = Enable embedded algorithm (mixed mode) |

**For Edge SLAM mode:** `{0x19, 0x95, 0x01, 0x01, 0x00}`
**For Mixed SLAM mode:** `{0x19, 0x95, 0x01, 0x01, 0x01}`

#### Start/Stop Edge Stream

Controls the SLAM data stream on endpoint 0x83.

```
Command:  [0x02, 0xA2, 0x33, edgeMode, rotationEnabled, flipped]
Response: [0x01, 0xA2, 0x33, ...]
```

| Parameter | Values | Description |
|-----------|--------|-------------|
| `edgeMode` | 0 or 1 | 1 = Start streaming, 0 = Stop |
| `rotationEnabled` | 0 or 1 | Include rotation matrix in packets |
| `flipped` | 0 or 1 | Flip coordinate system |

**To start edge SLAM:** `{0xa2, 0x33, 0x01, 0x00, 0x00}`

**Note:** libxvisio uses `{0xa2, 0x33, 0x01, 0x00, 0x00}` (rotation NOT enabled via this flag),
yet the stream still includes rotation data. The rotation field may always be present.

## Initialization Sequence

The correct order to initialize the XR50 for Edge SLAM streaming:

1. **Open USB device** and claim interface 3
2. **Read UUID** — verify device identity
3. **Read version** — verify firmware compatibility
4. **Read features** — check Edge mode is supported (bit 0)
5. **Configure device** — `{0x19, 0x95, 0x01, 0x01, 0x00}` for Edge mode
6. **Start edge stream** — `{0xa2, 0x33, 0x01, 0x00, 0x00}`
7. **Read interrupt EP 0x83** — SLAM packets arrive at ~950 Hz

**Important:** Steps 2-4 are optional for SLAM streaming. Steps 5-6 are mandatory.
Without step 5 (configure), the configure command returns all zeros but SLAM packets
still stream from EP 0x83 (the device appears to auto-start after the start edge command).

## SLAM Packet Format

SLAM data is received as 63-byte interrupt transfers on endpoint 0x83 at approximately
**950 Hz** (measured: 948.3 Hz over 2845 packets in 3 seconds).

### Byte Layout

```
Offset  Size   Type      Description
------  ----   ----      -----------
0       1      uint8     Response indicator (always 0x01)
1       1      uint8     Command echo byte 0 (0xA2)
2       1      uint8     Command echo byte 1 (0x33)
3-6     4      uint32    Edge timestamp (microseconds, little-endian)
7-10    4      int32     Translation X (little-endian, scaled)
11-14   4      int32     Translation Y (little-endian, scaled)
15-18   4      int32     Translation Z (little-endian, scaled)
19-20   2      int16     Quaternion W (little-endian, scaled)
21-22   2      int16     Quaternion X (little-endian, scaled)
23-24   2      int16     Quaternion Y (little-endian, scaled)
25-26   2      int16     Quaternion Z (little-endian, scaled)
27-36   10     varies    Extended rotation data (TBD, may be unused)
37-62   26     varies    Extended data (see below)
```

### Scale Factor

All translation and rotation values use the same fixed-point scale factor:

```
scale = 2^(-14) = 1/16384 = 6.103515625e-05
```

**Translation:** `float_meters = int32_raw * 6.103515625e-05`
**Quaternion:** `float_component = int16_raw * 6.103515625e-05`

### Coordinate System

- Translation is in meters, right-hand coordinate system (X=right, Y=up, Z=forward)
- Quaternion wire order is **[w, x, y, z]** (bytes 19-26), confirmed by C++ `libxvisio/src/slam.cpp`
- SDK convention stores as `[qx, qy, qz, qw]` (matching Android SDK `xv-types.h`)

### Extended Data (Bytes 37-62) — Partially Decoded

Based on analysis of stationary device data across multiple packets:

```
Offset  Size   Type      Hypothesis             Evidence
------  ----   ----      ----------             --------
37-38   2      int16     Accelerometer X?       Constant ~22176 (≈1.35g, gravity axis)
39-40   2      int16     Accelerometer Y?       ~125 (≈0.008g, near zero)
41-42   2      int16     Accelerometer Z?       ~-13 (≈-0.001g, near zero)
43-44   2      int16     Gyroscope X?           ~-14 (near zero, stationary)
45-46   2      int16     Gyroscope Y?           0 (near zero, stationary)
47-48   2      int16     Gyroscope Z?           0 (near zero, stationary)
49-50   2      int16     Unknown                0 (always zero in test)
51-52   2      int16     Unknown                Small values (3-4), noisy
53-54   2      int16     Unknown                Small values (8-12), noisy
55-56   2      int16     Unknown                Small values (±7), signed noise
57-58   2      int16     Confidence/Status?     Constant 16683 (≈1.018 scaled)
59-62   4      —         Padding                Always zero
```

The accelerometer hypothesis is based on:
- Scale factor 2^(-14) = 1/16384 corresponds to ±2g accelerometer at 16-bit resolution
- Value 22176/16384 ≈ 1.35g on one axis (gravity + sensor bias) is plausible
- Other axes near zero match a stationary device
- Further validation requires testing with device motion

### Timestamp

The edge timestamp is a uint32 counter in **microseconds**. Observed behavior:
- Consecutive packets are spaced by ~1000-1050 µs (matching ~950 Hz)
- Counter wraps around at 2^32 (~71.6 minutes)
- Example: ts=1596313963 → ts=1596314963 (delta = 1000 µs)

### Example Decoded Packet

Raw bytes:
```
01 a2 33 6b d1 25 5f 58 01 00 00 1e 00 00 00 c3
01 00 00 62 c0 3a 03 2d 06 5a fd 56 c0 f3 05 72
06 a9 05 6c 3f a0 56 7d 00 f3 ff f2 ff 00 00 00
00 00 00 04 00 09 00 07 00 2b 41 00 00 00 00
```

Decoded:
```
Timestamp:   1596313963 µs
Translation: [0.0210, 0.0018, 0.0275] m
Quaternion:  w=-0.9940, x=0.0504, y=0.0965, z=-0.0414  (wire bytes [19-26])
```

## Data Rates & Performance

| Metric | Measured Value |
|--------|---------------|
| SLAM packet rate | ~950 Hz |
| Packet size | 63 bytes |
| Data throughput | ~60 KB/s on EP 0x83 |
| Timestamp resolution | 1 µs |
| Inter-packet interval | ~1000-1050 µs |
| Translation resolution | 0.0610 mm (1 LSB) |
| Quaternion resolution | ~0.0061 (1 LSB at 2^-14) |

## Official SDK Architecture

### DLL Dependencies

```
xslam_sdk.dll              Main SDK (C++ mangled exports, 1821 symbols)
├── xslam_loader.dll       Dynamic module loader
├── xslam_algo_sdk.dll     SLAM algorithm
├── xslam_core.dll         Core utilities
├── xslam_log.dll          Logging
├── xslam_lib_ange.dll     Additional algorithms
├── xslam_surface_reconstruction.dll  3D reconstruction
└── xslam-drivers.dll      Hardware access (plain C hid_* exports)
    └── libusb-1.0.dll     USB transport (or Windows HID)
```

### Key API Functions (from DLL exports)

All `xslam_sdk.dll` functions use **C++ name mangling** (MSVC). They must be loaded
at runtime via `LoadLibrary` + `GetProcAddress` with exact mangled name strings.

| Function | Mangled Name |
|----------|-------------|
| `init_algorithm_and_loader()` | `?init_algorithm_and_loader@@YA?AW4xslam_status@@XZ` |
| `xslam_free()` | `?xslam_free@@YA?AW4xslam_status@@XZ` |
| `xslam_get_pose()` | `?xslam_get_pose@@YA?AW4xslam_status@@PEAUxslam_pose@@N@Z` |
| `xslam_get_pose_quaternion()` | `?xslam_get_pose_quaternion@@YA?AW4xslam_status@@PEAUxslam_pose_quaternion@@N@Z` |
| `xslam_start_edge_vo()` | `?xslam_start_edge_vo@@YA?AW4xslam_status@@XZ` |
| `xslam_stop()` | `?xslam_stop@@YA?AW4xslam_status@@XZ` |
| `xslam_hid_write_read()` | `?xslam_hid_write_read@@YA_NPEBEIPEAEI@Z` |
| `xslam_camera_is_detected()` | `?xslam_camera_is_detected@@YA_NXZ` |
| `xslam_wait_for_camera()` | `?xslam_wait_for_camera@@YA?AW4xslam_status@@XZ` |

### xslam-drivers.dll (HID Layer)

This DLL exports plain C functions compatible with the [HIDAPI](https://github.com/libusb/hidapi) interface:

- `hid_init()`, `hid_exit()`
- `hid_enumerate(vid, pid)`, `hid_free_enumeration()`
- `hid_open(vid, pid, serial)`, `hid_open_path(path)`, `hid_close()`
- `hid_write()`, `hid_read()`, `hid_read_timeout()`
- `hid_set_nonblocking()`
- `hid_get_manufacturer_string()`, `hid_get_product_string()`, `hid_get_serial_number_string()`
- `hid_error()`

## Known Issues & Observations

1. **SDK init failure:** `init_algorithm_and_loader()` detects the device but returns an error
   and segfaults on cleanup. May require specific device state, calibration data, or additional
   DLLs not present in the test environment.

2. **hidapi vs libusb:** The official `xslam-drivers.dll` uses Windows HID API (via hidapi),
   while `libxvisio` uses raw libusb control transfers. On Windows, `hid_read()` returns 0 bytes
   while raw libusb works perfectly. This suggests the official driver may use different
   HID report IDs or read mechanisms.

3. **Configure command:** Returns all zeros even when the device successfully starts streaming.
   The response data may not indicate success/failure.

4. **rotationEnabled flag:** The start edge stream command's `rotationEnabled` parameter
   appears to have no effect — rotation data is always present in the packets.

5. **Auto-streaming:** The device may begin streaming SLAM data as soon as the edge stream
   command is sent, without requiring a prior configure command. However, the configure step
   may affect the quality or mode of the SLAM algorithm on the device.

6. **Linux uvcvideo interference:** On Linux, the `uvcvideo` kernel driver automatically binds
   to the XR50's UVC camera interfaces (1 and 2), preventing the on-device edge SLAM from
   accessing its stereo cameras. Symptoms: SLAM outputs identity pose (all zeros) with near-zero
   confidence despite device motion. Fix: `sudo rmmod uvcvideo` before use. Permanent fix:
   `echo 'blacklist uvcvideo' | sudo tee /etc/modprobe.d/xr50-no-uvc.conf`. This does not
   affect Windows or macOS.

7. **Wire format correction:** Bytes [19-26] are a quaternion [w, x, y, z] as 4x int16 LE,
   NOT a 3x3 rotation matrix. This was confirmed by comparing with the C++ `libxvisio`
   driver (`slam.cpp`) which reads `reinterpret_cast<int16_t*>(&buffer[19])` as
   `{w, x, y, z}`. The initial PROTOCOL.md incorrectly documented these bytes as a
   row-major rotation matrix based on early reverse engineering.
