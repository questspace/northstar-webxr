# Northstar calibration workflow

This repo is aiming for a browser compositor that behaves like Project Esky's
low-level renderer: React Three Fiber, Three.js, or A-Frame renders scene
textures, then a Northstar-specific compositor presents stereo images on the
flat Windows display so they look correct after reflection through the headset
optics.

The browser is not the calibration authority. The calibration authority is a
set of measured transforms and optical warp maps that the browser consumes.

## What must be calibrated

Use one explicit transform chain and keep every value in meters, radians, and
right-handed Three.js coordinates after parsing:

1. `display_px -> eye_uv`
2. `eye_uv -> rectilinear_eye_ray`
3. `rectilinear_eye_ray -> reflected_glass_ray`
4. `left_eye/right_eye -> headset_eye_center`
5. `xr50_tracker -> headset_eye_center`
6. `ultraleap -> headset_eye_center`
7. `headset_eye_center -> world`

The current `visual-test/public/calibration/EskySettings.json` already has the
right broad shape:

- `v2CalibrationValues`: third-degree 2D polynomial warp coefficients.
- `v1CalibrationValues`: per-eye screen, eye, projection, and mesh/matrix data.
- `myOffsets`: Leap and tracker offsets from eye/headset space.
- `sensorModuleCalibrations`: camera intrinsics for preview/camera alignment.

## Recommended path for this hardware

Start with 2D optical calibration, then sensor extrinsics, then latency. Move
to 3D optical calibration when the headset mechanics are stable and repeatable.

### 1. Windows display baseline

- Set the Northstar display as an extended desktop, native resolution, no OS
  scaling, no HDR/night color transforms, and no browser zoom.
- Run the browser fullscreen on the headset display.
- Use a pure black clear color. Optical see-through AR depends on black pixels
  emitting no useful light, while bright objects reflect from the display into
  the combiner.
- Verify the physical panel layout before tuning optics: left half must go to
  the left optical path and right half to the right optical path. Fix mirroring
  and rotation before collecting calibration data.

### 2. Optical calibration, V2 first

V2 is the practical first target because it outputs the same coefficient family
that this repo already consumes. It uses gray-code structured light through the
headset, then fits a third-degree 2D polynomial from display UV to rectilinear
ray coordinates.

Capture requirements:

- A stereo camera looking through the headset from the expected eye positions.
- The camera must see the displayed gray-code patterns after reflection through
  the combiners. The Ultraleap Stereo IR 170 may work only if the display pattern
  is visible in its camera feed; if not, use a visible-light stereo camera or a
  RealSense-style calibration camera on a printed stand.
- Shield the rig from ambient light with cloth during capture.
- Capture width and height gray-code images for each eye.
- Fit coefficients and write `left_uv_to_rect_x`, `left_uv_to_rect_y`,
  `right_uv_to_rect_x`, and `right_uv_to_rect_y`.

Important coefficient order:

The Northstar V2 scripts emit numpy `polyvander2d` row-major coefficients:

```text
c00, c01*v, c02*v^2, c03*v^3,
c10*u, c11*u*v, c12*u*v^2, c13*u*v^3,
c20*u^2, ...
c33*u^3*v^3
```

The shader must evaluate coefficients in that order. If it treats index 1 as
`u` instead of `v`, the headset will look badly warped even with good data.

### 3. Optical calibration, V1/3D for final quality

V1/3D is more work but is the correct final target if you need multiple IPDs,
eye relief adjustment, and better edge quality.

It uses:

- Two calibrated stereo camera modules on a calibration rig.
- A known external flat monitor as a real-world reference.
- A ray-traced model of the display and optical combiner.
- An optimizer that compares the real monitor to an inverted virtual monitor
  reflected through the headset and minimizes the mismatch.

Use V1/3D when the frame, display mounts, and combiners are mechanically fixed.
Use V2 while iterating on hardware.

### 4. XR50 head tracker extrinsics

The runtime needs `T_head_from_xr50`: the rigid transform from XR50 SLAM tracker
space to the headset eye/head center.

Best path:

- If XR50 camera frames are available, use an ArUco/ChArUco marker visible to an
  external calibration camera and the XR50 cameras, then solve both poses to the
  same marker and infer `T_head_from_xr50`.
- If XR50 image access is not available yet, start with physical measurement and
  an interactive refinement scene: a world-locked cross/grid should stay fixed
  during head translation and yaw. Tune only this transform, not optical warp.
- Save the result into `myOffsets.TranslationFromTracker` and
  `myOffsets.RotationFromTracker`.

Do not use tracker offsets to compensate for optical distortion. If straight
lines bend, fix V2/V1 optics. If the whole world swims or has wrong parallax,
fix tracker extrinsics and pose prediction.

### 5. Ultraleap extrinsics

The runtime needs `T_head_from_leap`.

Two workable methods:

- Offline marker alignment: detect the same marker from a headset/eye
  calibration camera and from the Leap coordinate system if available.
- Online hand alignment: show a small sphere at the right index fingertip, freeze
  samples, physically match fingertip to sphere at many depths and palm rotations,
  then solve a Kabsch/Procrustes rigid transform.

The Northstar community Leap positioning tool uses the second approach. Collect
near, mid, and far samples; a transform that works only close to the sensor is
not good enough.

### 6. Browser compositor architecture

Keep app code engine-agnostic:

- Scene layer: R3F, raw Three.js, or A-Frame creates world content.
- Tracking layer: XR50 WebSocket and Ultraleap WebSocket produce timestamped
  poses in their native spaces.
- Calibration layer: JSON parser converts all calibration values into matrices.
- Compositor layer: renders left/right eye FBOs with asymmetric projections,
  applies the optical warp, then presents one fullscreen image on the headset
  display.

For `visual-test`, the current integration points are:

- `visual-test/src/lib/calibration.ts`: parse the Esky/Northstar calibration.
- `visual-test/src/components/NorthStarRenderer.tsx`: stereo render, V2 warp,
  and temporal reprojection.
- `visual-test/src/hooks/useXR50.ts`: XR50 pose stream.
- `visual-test/src/hooks/useUltraleap.ts`: Ultraleap hand stream.
- `visual-test/src/lib/transforms.ts`: temporary empirical axis/offset mapping.

The next cleanup should replace slider-based pose compensation in
`transforms.ts` with matrix composition from the calibration JSON. Sliders should
be a diagnostic override, not the source of truth.

### 7. Validation scenes

Run these in order after every calibration change:

1. Optical grid: straight horizontal, vertical, and diagonal lines. Check each
   eye separately, then both eyes.
2. Stereo convergence: one small cross at 0.25 m, 0.5 m, 1 m, and 2 m. It should
   fuse without eye strain.
3. Parallax: place a virtual cube on a physical table corner. Translate and yaw
   the head; the cube should stay on the corner.
4. Hand alignment: fingertip sphere should stay on the physical fingertip near,
   mid, and far from the Leap sensor.
5. Latency: quick yaw should not cause the scene to trail. If it does, add pose
   prediction before render and use temporal reprojection after render.

## Implementation backlog

1. Add a calibration importer that converts V2 `CalibrationResults.json` into
   this repo's `EskySettings.json` shape.
2. Add a browser diagnostic page for grid, convergence, parallax, and hand
   alignment tests.
3. Replace empirical pose sliders with `Matrix4` transforms from calibration.
4. Add timestamped XR50 velocity estimation and prediction.
5. Replace the current simplified homography reprojection with Esky's
   ray-plane method using the headset focal plane depth.
6. Add optional LUT texture support so future V1/3D or mesh-based calibrations
   can be consumed without changing shader code.

## Research notes

- Project Northstar calibration is necessary because handmade assemblies vary;
  the official docs describe optical calibration as compensating the distortion
  from the reflectors so normal images appear correct through the headset.
- The official docs distinguish 3D optical calibration from 2D optical
  calibration. 3D solves physical screen/combiner geometry and supports IPD/eye
  relief better; 2D solves a polynomial display-UV to rectilinear-ray map with a
  simpler single stereo camera setup.
- Project Esky adds the missing runtime pieces around calibration: sensor
  extrinsic calibration, One Euro filtering, pose prediction, asynchronous
  rendering, and temporal reprojection.
- Northstar optical descriptions vary between "parabolic reflector" in the
  official docs and ellipsoidal/birdbath descriptions in secondary summaries.
  Treat the actual combiner shape as build-specific and solve it empirically
  rather than hard-coding an ideal surface.

Sources:

- Project Esky paper: `paper-esky.pdf`
- Project Northstar calibration overview:
  https://docs.projectnorthstar.org/project-north-star/calibration/getting-started
- 2D optical calibration:
  https://docs.projectnorthstar.org/project-north-star/calibration/2d-calibration
- 3D optical calibration:
  https://docs.projectnorthstar.org/project-north-star/calibration/3d-calibration
- Northstar calibration system:
  https://docs.projectnorthstar.org/project-north-star/calibration/3d-calibration/bending-reality-north-stars-calibration-system
- Leap positioning tool:
  https://github.com/fmaurer/NorthStarToolbox
