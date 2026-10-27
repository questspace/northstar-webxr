# Official XVisio SDK on Windows (XVSDK 3.2.0)

Tested 2026-10-26 against this headset's XR50 (SN XR501G10002222006282, firmware
V1.04P31, features Edge + Mixed + Stereo; no RGB, ToF or on-device SGBM).

## Where it comes from

XVisio's public download server (`https://download.xvisiotech.com/sdk-download/`, a
plain directory listing; its TLS certificate has expired, so downloads use `curl -k` and
are pinned by SHA-256):

| File | Size | Notes |
|------|------|-------|
| `last_version/Select_Platform_Download/For windows/XVSDK-3.2.0-msvc2019-x64.exe` | 38.4 MiB | SDK build 3.2.0-20240905, the one we use |
| `.../XVSDK-3.2.0-msvc2017-x64.exe`, `XVSDK-Viewer-3.2.0-2-msvc2017-x64.exe` | 38.7 / 91.6 MiB | same SDK for VS2017, GUI viewer |
| `history_version/V3.2.0-20220408/.../For windows/zadig-2.3.exe` | 4.9 MiB | the vendor's driver step: WinUSB via Zadig |

`tools/xvsdk/fetch-xvsdk.ps1` downloads the SDK, checks the hash and unpacks the NSIS
installer into `tools/xvsdk/sdk` without running it (nothing is installed).

## What is inside

- `xvsdk.dll`: the C++ `xv::` API (`include/xvsdk/xv-sdk.h`, same API as the Android SDK in
  `xvisio-sdk/`, unmangled C++ headers this time).
- Transport backends: `xslam-hid-sdk.dll` (Windows HID), `xslam-uvc-sdk.dll` (fisheye
  cameras), `xslam-vsc-sdk.dll` (vendor bulk interface), `xslam-usb-sdk.dll`,
  `xslam-edge-sdk.dll`. Each links libusb 1.0.26 statically (WinUSB or UsbDk backend).
- `xvslam.dll` (21 MB): host-side "mixed" SLAM, stereo planes, CSLAM maps.
- `handskeleton_all*.dll`, `TNN.dll`: hand keypoints from the fisheye images.
- OpenCV 4.10, Ceres, MSVC runtime; samples `all_stream.exe`, `demo-api.exe` with source.

The API covers the whole roadmap item 5: `ImuSensor`, `FisheyeCameras` (with
anti-distortion), `SgbmCamera` (host stereo depth + `depthImageToPointCloud`),
`registerStereoPlanesCallback` (plane detection), `saveMapAndSwitchToCslam` /
`loadMapAndSwitchToCslam` (persistent maps), `GestureStream` (hand keypoints).

## Results with the stock Windows drivers

The XR50 enumerates as a composite device: interface 3 HID (Windows HID driver),
interface 1 "XVisio vSLAM" camera (Windows `usbvideo`), interface 0 "VSC interface"
(**no driver**, shows Error in Device Manager).

| Stream | Path | Result |
|--------|------|--------|
| Device info, features | HID | works |
| IMU (accel + gyro) | HID | **works, ~990 Hz**, gravity on +Y (~9.65 m/s²). Now also in `xvisio-rs` (see `PROTOCOL.md`, "IMU Packet Format") |
| Edge SLAM pose | HID | streams at ~990 Hz; zeros during the test because the headset lay still on the desk (our own driver read zeros at the same time) |
| Fisheye stereo, mixed SLAM, stereo planes, SGBM, CSLAM maps, hand keypoints | UVC through **DirectShow** (the SDK uses the videoInput library; no WinUSB needed) | **blocked by the headset hardware**: the XR50 drops off USB the moment its cameras start |

The camera is a plain UVC device that Windows already exposes: 640x800 NV12 @ 100 fps
(both eyes stacked) and 640x400 grey @ 100 fps; the SDK itself streams the stereo pair at
640x400 and ~25 fps. In this NorthStar the XR50 sits behind a Terminus FE1.1s USB 2.0 hub
board shared with the Ultraleap SIR170 (VID 1A40 PID 0101).

On the afternoon of 2026-10-26, starting the cameras (from ffmpeg at 100 fps, or the SDK)
knocked the XR50 off USB and it reset-looped for minutes. That evening the same SDK camera
run worked both with the XR50 on its own USB 3 port and again behind the headset's hub (0
drops in 60 s). The afternoon failures are best explained by the device already being in
its degraded state after repeated USB-C link drops, or by power with the headset on laptop
USB-C alone; the hub's bandwidth is not the limit for the SDK's camera mode. If the drops
come back, power the headset separately and power-cycle the XR50 before testing again.

Run it yourself (stop the hub's XR50 bridge first, the sensor has one owner):

```
cd tools\xvsdk\sdk\bin
all_stream.exe "" "rgb:0 rgb2:0 tof:0 fisheye:0 sgbm:0 eyetracking:0 slam:1 slam_edge:1 imu:1"
```

Two gotchas: start one SDK process at a time (a second one launched while the first is
still releasing the HID reads garbage: "HID uuid is not a string", "HID read flash
failed"), and expect "Failed to configure mode" / "Cannot load IMU calibration" warnings on
this firmware; IMU data still flows.

## Verified working (2026-10-26 evening)

On its own laptop USB 3 port and again behind the headset's hub, the stock drivers and
the official SDK give everything: mixed (host) SLAM at 500 Hz tracking real motion, the
stereo fisheye pair (640x400, ~26-30 fps, factory calibration for both cameras loaded),
feature keypoints, **stereo plane detection** (7 planes within two minutes of moving it) and
the IMU. The XR50 stayed connected throughout. Edge SLAM via `xvisio-rs` also tracks there
(confidence 0.97, 948 Hz), and the IMU stream (`Device::start_imu`) runs at ~950 Hz. The
firmware runs one HID stream at a time: edge SLAM poses or the raw IMU, never both.

## Drivers

The fisheye camera needs no driver change: both the SDK and a plain Rust UVC reader
(Media Foundation on Windows, V4L2 on Linux, AVFoundation on macOS) can read it once the
hardware can carry the stream. Interface 0 ("VSC interface", no driver) carries audio per
the SDK's device description, which the XR50 does not use here. The vendor's Zadig/WinUSB
step is therefore optional; if it is ever needed, install WinUSB on Interface 0 only
(Zadig: Options → List All Devices) and leave Interfaces 1 and 3 alone.

Acceptance test (passes in the headset as wired): `all_stream.exe "" "rgb:0 rgb2:0 tof:0
fisheye:1 sgbm:0 eyetracking:0 slam:1 slam_edge:0 imu:1 stereo_planes:1"` reports stereo
frames, mixed SLAM poses and plane updates while the headset is moved, without the device
dropping off USB.

## In the stack: the bridge's SDK source (Windows)

`xvsdk-bridge/` is a small C++ program on the official SDK (camera SLAM, stereo plane
detection) that prints JSON lines. Build it once:

```
powershell -ExecutionPolicy Bypass -File tools\xvsdk\fetch-xvsdk.ps1
powershell -ExecutionPolicy Bypass -File xvsdk-bridge\build.ps1
```

`build.ps1` generates the missing `xvsdk.lib` from the DLL's exports and builds with VS 2022
(`_DISABLE_CONSTEXPR_MUTEX_CONSTRUCTOR` is required: the SDK bundles an older
`msvcp140.dll`, on which `std::mutex` from VS 2022 17.10+ segfaults). The XR50 bridge then
runs it with `server --sdk` (hub: *XR50 source → SDK*): poses go out in the usual message,
converted to the raw XR50 convention, and plane updates as
`{"type":"planes","planes":[{"id","n":[..],"d","p":[[x,y,z],...]}]}` at 5 Hz, in the raw
XR50 world frame. The WebXR headset page draws them as outlines with a depth-only fill
(real surfaces hide holograms behind them); the native app draws the outlines.

The conversion was measured on the head (level, tilted down, turned left, with gravity from
the IMU): the SDK world is gravity-aligned with Y down like the raw world, and the SDK pose
is the IMU frame, which is the XR50 camera frame rotated 180° about Z. So
`q_raw = q_sdk · Rz(180°)`, translation and plane points unchanged.

| Source | Pose rate | Extras |
|--------|-----------|--------|
| edge (default, all platforms) | ~950 Hz, computed on the XR50 | none |
| sdk (Windows) | ~500 Hz, computed on the laptop | planes; fisheye images and maps are in the SDK, not yet forwarded |

### Room maps (persistent world frame)

With the SDK source the bridge also saves and reloads the SLAM map (`maps/room.xvmap`,
override with `XR50_MAP`): hub *Save room map* / *Load room map*, or `GET /map/save`,
`/map/load`, `/map/status` on the bridge (port 8080). A saved map is loaded automatically
whenever the SDK source starts; map events (`saved` with quality, `loaded`, `localized` with
the share of the view explained by the saved map) go to all clients. Verified 2026-10-26:
after a full restart with the map reloaded, the headset at the same resting spot reported
the same pose to 2 mm and 0.1° (a fresh start would have reported the origin), so WebXR
anchors placed before come back in place.

## Our own camera path (cross-platform, no SDK)

`xvisio-rs` reads the stereo fisheye images itself: `Device::start_slam_with_cameras`
sends the camera-enable commands with SLAM (configure `uvcMode=1`, stereo init/start from
libxvisio), and `camera::StereoCamera` (feature `camera`, via `nokhwa`: Media Foundation /
V4L2 / AVFoundation) reads the standard UVC stream: 640x800 NV12, the two cameras stacked,
640x400 each. Verified on Windows: 44 fps through `nokhwa` (100 fps with ffmpeg on the same
device) with edge SLAM at 976 Hz in the same process, unlike the SDK, which runs one SLAM
mode at a time. `cargo run --release --features camera --example fisheye`.

## Depth (our own, cross-platform)

`xvisio::stereo` computes depth on the host from the stereo pair (the XR50 has no depth
unit: SGBM off). Calibration comes from the device once through the SDK:

```
tools\xvsdk\sdk\bin\xvsdk-bridge.exe --dump-calibration > calibration\xr50-fisheye.json
cargo run --release --features camera --example depth
```

The device stores its own camera model, which the SDK only evaluates; `--dump-calibration`
writes each camera's pose in the IMU frame plus ~1,000 pixels raytraced by the SDK, and we
fit an Extended Unified Camera Model to them (0.36 / 0.40 px RMS on this unit). Baseline
49.8 mm. Each image is rectified into a shared upright pinhole view (320x240, 100° hFOV),
matched with a 7x7 census and 4-path semi-global matching (SGM, P1 8 / P2 96) over 48
disparities, filtered (uniqueness 1.1, left-right check, speckle filter) and refined to
sub-pixel. The top half of the camera
frame is calibration camera 0, which is the right camera as the cameras look.

| Measured 2026-10-26 | |
|---|---|
| Accuracy | 1.01 m at a measured 1.0 m (median of the centre 20x20 px) |
| Coverage | 43% of pixels over a 30 s room scan with SGM (14% with the earlier box filter) |
| Speed | 27-30 depth maps/s, 37 ms per map on the laptop CPU (rayon), poses unaffected |
| Range | useful to ~1.5 m: at 2.5 m the disparity is ~2.7 px and a quarter-pixel error is ±23 cm |

`camera::StereoCamera` drains the stream on its own thread and hands out only the newest
frame; reading frames slower than the camera's rate otherwise returns ones queued by the OS,
which measured ~180 ms stale.

## Reconstruction (our own, cross-platform)

`xvisio::recon` fuses the depth with edge-SLAM poses into a sparse voxel-hashed TSDF
(VoxBlox-style 8³ blocks, 2 cm voxels, 1/z² weights, depth up to 1.5 m), meshes it
incrementally with Surface Nets and finds horizontal and vertical planes (normal/offset vote,
least squares on the largest connected region, occupied area). The pose and the depth frame
are paired with a 60 ms camera delay, and the depth camera is mounted in the IMU frame that
the edge pose reports (identity). Both were fitted on recorded data:

```
cargo run --release --features camera --example recon_record -- 30 ../calibration/xr50-fisheye.json scan
cargo run --release --example recon_eval -- scan scan/room.ply   # mounting, latency, mesh, planes
cargo run --release --features camera --example reconstruct      # live
```

`recon_eval` carries each frame's depth into a later frame with the SLAM motion and compares
it with what that frame measured: 14 mm median (vs 28 mm ignoring the motion; the next-best
mounting 23 mm). On a 30 s scan: desk and floor found at plausible heights. Live: ~20 fused
frames/s, fusion 1 ms, remesh 2 ms, planes 4 ms; depth (45 ms) is the bottleneck.
In the stack: `server --recon` (hub source "Edge SLAM + room mesh and planes") runs this
live and streams changed mesh blocks once a second as binary WebSocket messages
(`xvisio::recon::map` wire format; a new client gets every block) and the planes in the SDK
source's JSON. The WebXR runtime exposes them through IWER's environment module as
`frame.detectedPlanes`, `frame.detectedMeshes` and hit tests against the planes.
Tracking needs confidence: when the XR50 reports ~0 even with the cameras off it is stuck
and needs a USB re-plug (`examples/camera_modes.rs` checks motion and confidence).

`examples/stereo_diag.rs` counts consistent matches for every camera/frame pairing; the
right one stands out (about 20,000 vs 8,000 matches).

## What this means for our stack

- The official SDK is the ground truth we were missing, and it needs no disassembly to use
  that way: Windows hands every HID input report to every open handle, so
  `xvisio-rs/examples/hid_sniff.rs` records the raw reports while the SDK prints decoded
  values, joined on the device timestamp; command bytes are then found by searching
  `xslam-hid-sdk.dll` for the report id. That is how the IMU stream (`BD DF`) was decoded
  and added to `xvisio-rs` (`examples/imu_probe.rs`: 995-998 Hz). The fisheye path is
  standard UVC, so the cross-platform Rust plan for it is a UVC reader, not a USB protocol.
- On Windows it can also be used directly (a small bridge built on `xvsdk.dll`) for
  planes, depth and maps before our own driver implements them. On macOS and Linux the
  same API exists as `.deb` packages for Ubuntu; macOS has no official build.
