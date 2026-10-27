# Vibestar roadmap

Vibestar is an open, modular XR stack for Project NorthStar-class optical see-through
headsets: tracking bridges, a WebXR runtime, a native pipeline and a device hub, all in
this monorepo. The direction below is the reason the stack exists.

Status legend: **done** (verified on the head), **built** (works in tests, not yet checked
on the head), **next**, **later**.

## At a glance

| # | Item | Status |
|---|------|--------|
| 1 | Developer loop on the headset (hub, hot reload) | done |
| 2 | Hands as the interface | built, hand-feel to check |
| 3 | Seeing Spaces workbench | built, to check on the head |
| 4 | QuestSpace pilot | later, blocked on 5 |
| 5 | The whole XR50 on every desktop | **in progress**: IMU, camera SLAM, planes on Windows |
| 6 | Reliability of the headset link | next (hardware: own power supply) |

Next up, in order: **6** (power the headset separately, confirm the recovery path), then
**5.1 to 5.3** (protocol capture, stereo/IMU, CSLAM maps), which unblock persistent anchors,
depth, planes and the QuestSpace multi-user scene.

## 1. Developer loop on the headset (done)

- `northstar-hub` (http://localhost:9100): keeps the headset display laid out (laptop
  primary, panel right at native 2880x1600 @ 89 Hz, rotated 180° as the optics fit needs),
  runs the tracking bridges, launches the native app or a WebXR page (selector: headset,
  workbench, menu, hands, cones) onto the panel on the discrete GPU, and hot-reloads the
  native app while its source changes. WebXR hot-reloads through Vite.
- An agent (or a person) edits `visual-test/src` or `northstar-native/src` and sees the
  change in the headset within seconds, with the laptop screen untouched.
- Verified frames and optics: both XR50 frames are camera convention, Three = `Rx180 · q ·
  Rx180` (`PROTOCOL.md`). Native and WebXR agree and were checked on the head.

## 2. Hands as the interface (built, hand-feel to check)

- WebXR interaction layer (`visual-test/src/xr/interaction.ts`): pinch to grab (one hand
  moves and turns, two hands scale), fingertip presses, slider grabs, hover; works on any
  WebXR device with hands. UI kit (`ui.ts`): buttons, a palm-summoned menu, sliders. Hand
  occluders from the joints (`hands.ts`). Demo: `/examples/menu.html`.
- Native: the same grab / two-hand scale / press rules and depth-only occluders in
  `northstar-native/src/interact.rs` (cubes to grab, a Reset button, fingertip cursors).
  Leap hands decode at 90 Hz after the LeapC struct-packing fix.
- Persistent placement through WebXR anchors (`anchors.ts`): objects return after a page
  reload while the bridge keeps the SLAM frame alive. Across a headset power cycle the
  frame restarts; CSLAM map save/load (item 5) is the fix.
- A mouse-driven synthetic hand (`?hand=mouse`) exercised all of it without the headset.
- **Left to do:** wear it. Check occluder size, pinch thresholds, menu summon angle and
  the hand offset calibration (the fingertip dot was offset). Constants sit at the top of
  `interaction.ts`, `hands.ts`, `ui.ts` and the hand sliders in the calibration panel.

## 3. Seeing Spaces workbench (built: `docs/WORKBENCH.md`)

A simulated Physics-AI workbench after Bret Victor's *Seeing Spaces*
(https://worrydream.com/SeeingSpaces/): a self-balancing robot on a bench with the
debugging tools he describes as hand-operated instruments around it: seeing inside (every
sensor and controller variable, placed where it lives), seeing across time (scrub the last
ten seconds, pin and compare moments), seeing across possibilities (a Kp x Kd sweep of the
last push you can touch). `/examples/workbench.html` starts its session by itself on the
headset.
- **Left to do:** try it on the head. Then a learned policy in place of the PID, and real
  hardware streaming its telemetry into the same frame record.

## 4. QuestSpace pilot (later)

An AI+XR environment for the QuestSpace makerspace in Kosice: on-demand digital twins of
machines and projects, shared multi-user space (several NorthStars or AI glasses see the
same anchored scene), Dynamicland-style spatial programming with AI agents editing the
running scene. Prerequisites: a reliable hands-first interface (2), persistent anchors
across devices and sessions (5.3), and a device hub that makes a headset as easy to attach
as a monitor (1, 6).

## 5. The whole XR50, on every desktop (next)

Today the Rust driver (`xvisio-rs`) speaks the XR50's HID control channel and reads the
6DOF stream (~950 Hz, Windows via hidapi). The sensor has more, and the official SDK
(`xvisio-sdk/xv-sdk.h`) shows the surface: stereo fisheye images, IMU, SGBM depth
(host-side stereo matching), ToF on units that carry it, CSLAM map save/load and
relocalisation, and the "mixed" mode that fuses them. This unit reports only
EDGE_MODE | MIXED_MODE | STEREO, so depth is host-side stereo. Windows must support all of
it first; Linux and macOS follow.

**Done (2026-10-26):** the official Windows SDK (XVSDK 3.2.0) runs every camera feature on
this headset as wired, with stock drivers (`docs/XR50_OFFICIAL_SDK.md`). `xvisio-rs` reads
the full-rate IMU (`Device::start_imu`; the firmware runs edge SLAM or the IMU, one at a
time). The XR50 bridge has an SDK source (`server --sdk`, hub *XR50 source*) that brings
camera SLAM and **stereo plane detection** into the WebXR runtime and the native app.
The camera turned out to be plain UVC, and `xvisio-rs` now reads it itself
(`start_slam_with_cameras` + `camera::StereoCamera`, feature `camera`), alongside edge SLAM:
steps 1-2 below are done on Windows (to verify on Linux/macOS). Room maps (step 3) work
through the SDK source: save once, auto-loaded on start, world frame restored to ~2 mm.
Step 4 (depth) is done in Rust, cross-platform: `xvisio::stereo` (census + SGM), 1.01 m
measured at 1.0 m, ~28 maps/s. Step 5 runs offline and live in Rust (`xvisio::recon`: TSDF,
Surface Nets mesh, horizontal/vertical planes; mounting and camera delay fitted on recorded
data) and is in the stack: hub *XR50 source* "Edge SLAM + room mesh and planes" (`server
--recon`) streams mesh blocks and planes; WebXR apps get them as `plane-detection`,
`mesh-detection` and `hit-test` (IWER environment module, `src/xr/environment.ts`), the
headset view uses the mesh as an occluder (`?mesh` shows it), the native app draws the
planes. Next: our own map format; learned stereo depth would extend range and coverage.

1. **Capture the protocol, not guess it.** Run the official Windows DLL (`xslam_sdk.dll`,
   mangled exports in `xslam-headers/xslam_sdk.h`) under USBPcap while calling each feature
   once: stereo stream, IMU, SGBM, map save/load. The captures become `xvisio-rs` commands
   with tests against the recorded bytes.
2. **Stereo images + IMU in `xvisio-rs`** (bulk endpoints next to the 0x83 interrupt
   stream). Everything below builds on it. Verify on Windows, then Linux, then macOS.
3. **CSLAM map save/load.** Anchors survive a headset power cycle; several headsets
   relocalise into one map (QuestSpace prerequisite).
4. **Depth.** SGBM on the host from the stereo pair (10-20 Hz, coarse but enough for a
   table and walls), the device's own SGBM/ToF where a unit has it.
5. **Planes and meshes** from the depth: plane fitting (RANSAC on the point cloud, tracked
   over frames) and TSDF fusion for a mesh, in the SLAM frame. Exposed through the WebXR
   plane-detection and mesh-detection APIs in the runtime and as a native module, so apps
   written against Quest's APIs run unchanged; occlusion then needs no hand-placed planes.
6. **macOS parity.** The pose is read there but SLAM stays non-functional (`confidence
   ~0`); the step 1 capture shows the start-up handshake the official SDK sends, the likely
   missing piece. Linux parity falls out of the libusb path.

Honest expectations: host-side stereo depth gives centimetre-rough surfaces, fine for "the
cube sits on the table" and occlusion by large objects, not for fingertip-accurate contact;
a dedicated depth camera would be the upgrade if the workbench needs it.

## 6. Reliability of the headset link (next)

Powered from one laptop USB-C port, the whole link (panel + XR50 + Leap) drops every few
minutes: the panel re-enumerates (rotation lost, windows parked on it), the XR50 comes back
degraded, the Ultraleap service freezes.
- **Done:** the hub re-applies layout and rotation, relaunches native/kiosk on the panel's
  re-attach, sweeps stray windows back to the laptop, and (started with
  `northstar-hub/run-elevated.cmd`) restarts the Ultraleap service and re-plugs the XR50
  without UAC prompts; USB selective suspend is disabled.
- **Next:** feed the headset from its own power supply (the actual fix), then confirm the
  recovery path end to end. Optional: a hub-managed scheduled task that starts the hub
  elevated at login.
