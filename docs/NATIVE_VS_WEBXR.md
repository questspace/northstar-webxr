# Native vs WebXR on this NorthStar

Two pipelines render the same scene (grid, eye-level anchors, 20 floating cubes, Leap hand
joints) through the same optics model:

| | WebXR (`visual-test`, `#headset`) | Native (`northstar-native`) |
|---|---|---|
| Head pose | XR50 → `xvisio-rs` bridge → WebSocket JSON (120 Hz) → browser → IWER runtime | XR50 → `xvisio` crate in-process (~950 Hz) |
| Hands | Ultraleap-Tracking-WS bridge → WebSocket JSON → `XRHand` joints | LeapC directly (`src/leapc.rs`) |
| Render | three.js via `navigator.xr`, side-by-side on the canvas | wgpu, side-by-side texture |
| Optics | `src/xr/optics.ts` (GLSL, raw-WebGL post pass) | `src/shaders/warp.wgsl` (same math) |
| Re-projection | newest pose at the warp pass | newest pose at the warp pass |
| Present | browser `requestAnimationFrame` | swapchain, Fifo, 1 frame in flight |

## Measured (2026-10-25, windowed on the 60 Hz desktop monitor, headset on the desk)

"Pose age" = time from the tracker sample leaving the pose source to the moment the warped
frame is submitted. Both pipelines print it once a second (`[XR] …` in the browser console,
`[native] …` on stdout).

| | fps | worst frame | pose age avg | pose age max |
|---|---|---|---|---|
| WebXR, bridge at 60 Hz (before) | 60 | 18–25 ms | 8–16 ms | 37 ms |
| WebXR, bridge at 120 Hz | 60 | 17.6–18.3 ms | **6.0 ms** | **11 ms** |
| Native, pose sampled before vsync wait (first version) | 60 | 17.5 ms | 17 ms | 34 ms |
| Native, pose sampled after swapchain acquire | 60 | 17.5 ms | **1.3 ms** | **4 ms** |

On the 120 Hz panel the frame numbers halve; the pose-age gap stays: the native path can read
the 950 Hz stream ~1 ms before the frame is composed, the browser gets a sample every 8 ms over a
socket and cannot block on vsync.

## What the native pipeline taught us (applied / to apply in WebXR)

1. **Where you sample the pose matters more than how fast it arrives.** The first native build
   sampled the pose, then blocked on vsync in `get_current_texture()`: 17 ms of age with a 1 ms
   stream. Acquiring the swapchain image first dropped it to 1.3 ms. The browser equivalent is to
   do as little as possible between the pose read and the warp: the runtime already re-reads the
   newest pose at the warp pass (`attachOptics` in `src/xr/runtime.ts`).
2. **Bridge throttle = latency.** 60 → 120 Hz broadcast halved WebXR pose age (16 → 6 ms avg).
   Applied in `xvisio-rs/examples/server.rs`. Going to the full 950 Hz would cost main-thread
   JSON parsing; a binary frame on a worker would be the next step.
3. **Predict, don't just latch.** With 6 ms (web) / 1.3 ms (native) of age plus ~half a refresh
   to scan-out, extrapolating orientation by angular velocity over that horizon is the remaining
   win, and it is cheap on both sides because the XR50 stream carries device timestamps. Not done
   yet — it belongs in `transforms.ts` / `transforms.rs` so both pipelines share it.
4. **One frame in flight.** `desired_maximum_frame_latency: 1` keeps the native queue shallow;
   the browser compositor adds at least one more frame the page cannot remove. That fixed cost
   is the structural WebXR disadvantage; everything else is reachable from JS.
5. **DX12, not Vulkan, on this laptop.** The Vulkan path exits silently after device creation on
   the Intel iGPU + RTX 3080 combination; the native app defaults to DX12.

## Where it landed

- The XR50 can only have one owner: handing it between the bridge and the native app
  made it re-enumerate into a degraded state (empty identity, garbage poses) that only a
  USB re-plug clears. The bridge is therefore the single owner, and the native app is
  its client at the full 950 Hz (`ws://localhost:8080/?rate=full`); `--xr50 direct` keeps
  the in-process path for measurements.
- On this laptop the panel's DisplayPort hangs off the Intel iGPU, so only the iGPU can
  present to it (wgpu picks it regardless of power preference). The per-pixel polynomial
  moved into a precomputed ray lookup texture; at native 2880×1600 the iGPU now runs
  53–60 fps with 2 frames in flight (the remaining cost is the two eye passes on an
  outdated Intel driver).
- `northstar-hub` runs all of this: bridges, layout, launch on the headset, hot reload.

## Running

```
cd northstar-native
cargo run --release              # fullscreen on the panel (DisplayXLoc/YLoc from the calibration)
cargo run --release -- --windowed --no-leap
```

Stop `start-all.ps1` / the XR50 bridge first: two processes reading the XR50 interrupt endpoint
split its reports. Keys: `U` warp, `R` re-projection, `X`/`Y` flip the warp axes, `Esc` quit.
