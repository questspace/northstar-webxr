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
