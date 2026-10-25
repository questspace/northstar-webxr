# WebXR on Project NorthStar — plan

Goal: a real `navigator.xr` on this NorthStar (optical see-through, Ultraleap hands,
XVisio XR50 6DoF) so that any WebXR app — three.js `renderer.xr`, A-Frame, Babylon,
react-three/xr — runs unchanged with `immersive-ar`, hand tracking and anchors.

## Why a browser-side runtime, not an OpenXR one

- Chrome on Windows only exposes WebXR through an OpenXR runtime, and only one that
  advertises `XR_EXT_win32_appcontainer_compatible`; AR on Windows is behind a flag.
- No OpenXR runtime drives this hardware on Windows. Monado's North Star driver is
  Linux-only and built around the RealSense T265; there is no XVisio driver anywhere.
- Writing an OpenXR runtime (Vulkan/D3D swapchains, the loader, hand-tracking
  extension) is an order of magnitude more work than a JS runtime, and we already own
  the tracking bridges as WebSockets.
- 8th Wall (open-sourced Feb 2026, MIT) ships the engine framework only: SLAM is a
  closed binary, hand tracking and HMD output are not included, and it targets phone
  cameras. It brings nothing for a headset with native 6DoF. Not used.
- The immersive-web `webxr-polyfill` is VR-only (no AR, hands or anchors). Not used.

## Base: fork Meta's IWER (`iwer` 2.7.0, MIT)

IWER is a complete TypeScript implementation of the WebXR Device API meant for
emulation, installed with `new XRDevice(config).installRuntime()`. Verified from the
package typings:

- Session modes `inline | immersive-vr | immersive-ar`; `environmentBlendModes` per
  mode in the device config → we declare `immersive-ar: 'additive'` (optical
  see-through: black = transparent).
- Features: `local`, `local-floor`, `unbounded`, `anchors`, `hit-test`,
  `plane-detection`, `mesh-detection`, `hand-tracking`, `depth-sensing`.
- `XRDevice.position/quaternion` and `controlMode = 'programmatic'` let the page drive
  the headset pose every frame.
- `XRHandInput` with all 25 WebXR joints, `select` events from a pinch value.
- `XRAnchor.requestPersistentHandle()`, `XRSession.persistentAnchors`,
  `restorePersistentAnchor()` already exist (localStorage-backed).

**Status (2026-10-25): phases 0–2 are implemented without a fork.** IWER exports its
private symbols (`P_DEVICE`, `P_SESSION`, `P_SPACE`, `P_JOINT_SPACE`), so
`visual-test/src/xr/runtime.ts` adapts a stock `iwer` dependency: it replaces
`updateViews`, `onBaseLayerSet`, `onFrameStart` on the device and `getProjectionMatrix`,
`onDeviceFrame` on each session. The optics live in `visual-test/src/xr/optics.ts`
(Esky-faithful: 70° eye cameras, polynomial → ray → eye texel, raw-WebGL post pass with
full state restore). Example pages: `/examples/cones.html`, `/examples/hands.html`.

What the adapter changes (everything else stays upstream):

1. **Views**: per-eye projection matrices from `cameraProjection` tangent half-angles
   and eye positions from `eyePosition` (replaces `fovy`/`ipd`).
2. **Output**: after the app renders into the `XRWebGLLayer` framebuffer, run the
   NorthStar polynomial undistortion + temporal re-projection pass (the shaders in
   `NorthStarRenderer.tsx`) into the 2880×1600 canvas. The app never sees the optics.
3. **Hands**: a `setJointTransforms()` path on `XRHandInput` fed from Leap v6 JSON
   (`mcp/pip/dip/tip` + `wrist`/`palm`, mm → m, through `leapToHead()`), pinch from
   `pinchStrength`. IWER's pose-id hands remain for the emulator.
4. **Tracking input**: head pose from `xr50ToThree()` (`transforms.ts`), filtered.

Deliverable: `visual-test/src/xr/` exporting `installNorthStarXR(calibration)`;
`#headset` becomes a plain WebXR three.js app (`renderer.xr.enabled = true`), and any
stock three.js WebXR example must run on the headset as the acceptance test.

A native Rust/wgpu twin of the pipeline lives in `northstar-native/`; measurements and the
latency lessons it produced are in `docs/NATIVE_VS_WEBXR.md`.

## Pass-through, culling, occlusion

- Pass-through is the optics: additive display, clear colour black. Nothing to stream.
- Frustum culling is three.js default. For occlusion we have no depth sensor (this
  XR50 reports `EDGE_MODE | MIXED_MODE | STEREO` only, no SGBM/ToF), so:
  - **Hand occlusion**: render a hand mesh from the Leap joints with
    `colorWrite = false` (depth only) before the scene → holograms behind the real
    hand are cut out, which on an additive display reads as the hand being in front.
  - **Room occlusion**: user-placed occluder planes (floor, desk, wall) pinned with
    a pinch, saved as anchors. Exposed to apps through IWER's plane-detection / depth
    APIs so WebXR occlusion code paths keep working.

## Persistent object placement

Within a session, anchors are poses in the SLAM world frame and are stable.
Across sessions the SLAM origin must be reproducible:

- **Now**: a dock routine — the headset starts from a marked spot on the desk, so the
  origin repeats to a few centimetres; anchors live in IndexedDB keyed by a room id.
- **Proper**: XR50 CSLAM map save/load (`saveMapAndSwitchToCslam` /
  `loadMapAndSwitchToCslam` in `xv-sdk.h`) gives relocalisation into a stored map.
  The HID commands are undocumented; capture them from the official
  `xslam_sdk.dll` on Windows (USBPcap while calling the mangled exports listed in
  `xslam-headers/xslam_sdk.h`) and add them to `xvisio-rs`. Then `restorePersistentAnchor`
  works after a restart with no dock.

## Phases

| # | Scope | Done when |
|---|-------|-----------|
| 0 | Coordinate pipeline (this branch): quaternion poses, Unity→Three calibration, correct frustum, no CSS mirror | Head and hands move correctly on the panel |
| 1 | IWER fork + NorthStar device: views, output pass, pose feed, `immersive-ar` additive | three.js `webxr_ar_cones` example runs on the headset |
| 2 | Hands: XRHand joints from Leap, pinch select, depth-only occluder mesh | `webxr_xr_handinput` example works; hand cuts holograms |
| 3 | Anchors: pinch-to-place, persistent anchors in IndexedDB, dock-based origin, occluder planes | Objects return after a page reload |
| 4 | CSLAM map save/load in `xvisio-rs` | Objects return after a headset restart without docking |
