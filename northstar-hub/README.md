# northstar-hub

The device manager for a Project NorthStar connected over USB-C: open
http://localhost:9100 and you get

- **Headset display**: detects the panel by its EDID (`OPD2C02`), and keeps the desktop
  sane — laptop primary at the origin, panel to its right at native 2880×1600 @ 89 Hz.
  Auto-fix runs every 2 s, so plugging the headset in no longer moves your windows.
- **Trackers**: XR50 and Ultraleap on USB, Ultraleap service state, one-click service
  restart and a software re-plug of the XR50 (both need a UAC prompt). The XR50 bridge
  is the single owner of the sensor; the native app and the WebXR page are its clients
  (native at the full 950 Hz via `ws://localhost:8080/?rate=full`).
- **Services**: XR50 WebSocket bridge, Leap WebSocket bridge, Vite dev server, each
  with a log tail.
- **Run on headset**: launch the native XR app or the WebXR page (Chrome/Edge kiosk in
  its own profile) onto the panel, and close them from the laptop. The native app and
  the XR50 bridge both open the sensor, so the hub swaps them.
- **Native hot reload**: watches `northstar-native/src`, rebuilds and relaunches on the
  headset after every successful build. WebXR hot-reloads through Vite already.

```
cd northstar-hub
cargo run --release              # repo root is found from the exe location (or VIBESTAR_ROOT)
cargo run --release -- --no-auto-display
```

API (all local): `GET /api/status`, `POST /api/display/fix`, `POST /api/<xr50|leap|web|native|kiosk>/<start|stop>`,
`POST /api/all/<start|stop>`, `POST /api/leap-service/restart`, `POST /api/xr50/reset-usb`, `POST /api/dev/native/<on|off>`.

## Debugging the kiosk page

The kiosk runs with `--remote-debugging-port=9222`, so its console and page state are
readable without touching the headset:

```
node northstar-hub/kiosk-console.mjs "document.title"            # console tail + evaluated expression
RELOAD=1 node northstar-hub/kiosk-console.mjs "location.href" 8000   # reload first, wait 8 s
```

The `[XR] … fps … pose age` lines are the health check: 60 fps and ≤10 ms pose age is right.
The kiosk is launched with `--force_high_performance_gpu`; on the Intel iGPU the same page
drops to ~50 fps with 40 ms pose age and every head turn lags.
