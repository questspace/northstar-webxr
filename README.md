# Vibestar

Project NorthStar workspace with XR50 + Ultraleap integration experiments.

## Current Runtime Policy

- **XR50 SLAM runtime:** run on **Linux or Windows**.
- **macOS:** development/UI environment only for now.
- **Why:** macOS receives XR50 packets at high rate, but tracking stays non-functional (`confidence ~0.001`, translation fixed at zero) across tested startup sequences/backends.

See `xvisio-rs/SETUP.md` for the detailed status and commands.

## Hardware Status (Latest)

| Component | Model | Status |
|-----------|-------|--------|
| 6DOF Tracking | XVisio SeerSense XR50 | ✅ Working on Linux/Windows, ❌ not production-ready on macOS |
| Hand Tracking | Ultraleap Stereo IR 170 | ✅ Working |
| Display | 2880×1600 @ 89Hz | ✅ Detected |

## Recommended Setup

1. Connect XR50 runtime host (Linux/Windows) to XR50 and run `xvisio-rs`.
2. Keep macOS for rendering/dev tooling.
3. Stream pose data from runtime host to macOS app over network.

## Repository Pointers

- `xvisio-rs/` — active cross-platform XR50 SDK work.
- `visual-test/` — frontend visualizations.
- `MACOS_FIXES.md` — archived macOS investigation notes and findings.

## NorthStar hub (start here)

`northstar-hub` is the device manager: `cd northstar-hub && cargo run --release`, then
open http://localhost:9100. It keeps the headset display laid out (laptop primary, panel
to the right at native 2880×1600 @ 89 Hz, auto-corrected on plug), runs the tracking
bridges, launches the native XR app or the WebXR page onto the headset, closes them,
restarts the Ultraleap service / re-plugs the XR50 over USB, and hot-reloads the native
app from `northstar-native/src`. WebXR hot-reloads through Vite. The *WebXR page* selector
picks what the kiosk opens on the headset: the headset scene, the Seeing Spaces workbench
(`docs/WORKBENCH.md`), the hand menu demo, or the stock three.js examples. `start-all.ps1`
is the older bridge-only launcher. Design notes: `docs/WEBXR_PLAN.md`,
`docs/NATIVE_VS_WEBXR.md`, `docs/ROADMAP.md`.

Disable USB selective suspend once (admin), or the headset's USB hub gets put to sleep
mid-session and both trackers drop:
`powercfg /setacvalueindex SCHEME_CURRENT 2a737441-1930-4402-8d77-b2bebba308a3 48e6b7a6-50f5-4782-a5d4-53bb8f07e226 0`
(and `/setdcvalueindex`), then `powercfg /setactive SCHEME_CURRENT`.

## Known Issue: sensors die after PC sleep (USB selective suspend)

**Symptom.** After the machine sleeps/resumes (or after long idle), one or both
sensors silently stop feeding the app:

- **Ultraleap:** app shows connected (green) but no hands; the bridge logs
  `0 frames/s`. Ultraleap's own log (`C:\ProgramData\Ultraleap\HandTracker\Logs\tracker_log.txt`)
  shows `Broken device detected, missing frame data` → `Ejecting device` →
  `Power Management - System Resuming` → `Stopping Server`. The
  `UltraleapTracking` service shuts itself down on the broken-device event and
  does **not** come back up on its own.
- **XR50:** pose stops updating; the bridge re-opens the device in a degraded
  state (empty UUID/features) after the USB re-enumeration.

**Root cause.** Windows **USB selective suspend** powers down the sensor USB
ports during sleep/idle. On resume the devices re-enumerate, which the drivers
treat as a hard disconnect. This is the shared root cause for *both* sensors.

**Manual recovery (current workaround).**
1. Ultraleap: start the service from an **admin** shell —
   `Start-Service UltraleapTracking` (or `services.msc` → *Ultraleap Tracking
   Service* → Start). Then restart the v6 bridge so it makes a fresh LeapC
   connection (re-run `start-all.ps1`, or restart just the bridge process).
2. XR50: the bridge already self-heals (see `xvisio-rs/examples/server.rs`
   degraded-device re-init) and the frontend has a stall watchdog
   (`visual-test/src/hooks/useXR50.ts`).

**Potential fixes (not yet implemented).**
- **Prevent it:** disable USB selective suspend for the Leap + XR50 ports (or
  globally on this dev box). Device Manager → each USB Root Hub → *Power
  Management* → uncheck "Allow the computer to turn off this device", **or**
  Control Panel → Power Options → advanced → *USB settings → USB selective
  suspend setting → Disabled*. A per-device driver flag
  (`Set-PnpDeviceProperty` on `DEVPKEY_..._SelectiveSuspendEnabled`) is also
  possible but hardware-specific.
- **Recover automatically:** add a lightweight watchdog to `start-all.ps1` that,
  if `UltraleapTracking` is `Stopped` (or the bridge logs `0 frames/s` for a few
  seconds), restarts the service + bridge — mirroring the XR50 self-healing that
  already exists. Note the service restart needs elevation (one UAC prompt, or
  run `start-all.ps1` from an admin shell).
