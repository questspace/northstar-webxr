//! NorthStar hub: a local GUI (http://localhost:9100) that keeps the headset display
//! laid out, runs the tracking bridges, launches the native app or the WebXR page onto
//! the headset, and hot-reloads the native app while you edit it.
//!
//!   cargo run --release            (from northstar-hub/, repo root is found automatically)
//!   cargo run --release -- --no-auto-display
mod devices;
mod display;
mod processes;
mod watch;
mod windows;

use processes::{Processes, Spec};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, Server};

const PORT: u16 = 9100;
/// WebXR pages the kiosk can open on the headset: (name, path on the Vite server).
const KIOSK_PAGES: &[(&str, &str)] = &[
    ("headset", "/#headset"),
    ("workbench", "/examples/workbench.html"),
    ("menu", "/examples/menu.html"),
    ("hands", "/examples/hands.html"),
    ("cones", "/examples/cones.html"),
];
const UI: &str = include_str!("ui.html");

struct Hub {
    procs: Processes,
    dev: Arc<watch::DevState>,
    /// Which WebXR page the kiosk opens (a KIOSK_PAGES name)
    page: Mutex<String>,
    /// XR50 pose source: "edge" (on-device SLAM via xvisio-rs), "recon" (edge SLAM plus our own
    /// room mesh and planes, `server --recon`) or "sdk" (host SLAM + planes via the official
    /// SDK, `server --sdk`)
    xr50_source: Mutex<String>,
}

impl Hub {
    fn root(&self) -> &std::path::Path {
        self.procs.root()
    }

    fn panel(&self) -> Option<display::Display> {
        display::list().into_iter().find(|d| d.is_panel && d.attached)
    }

    fn spec(&self, name: &str) -> Result<Spec, String> {
        let root = self.root();
        Ok(match name {
            "xr50" => Spec {
                program: root.join("xvisio-rs/target/release/examples/server.exe"),
                args: match self.xr50_source.lock().unwrap().as_str() {
                    "edge" => vec![],
                    source => vec![format!("--{source}")],
                },
                cwd: root.join("xvisio-rs"),
            },
            "leap" => Spec {
                program: root.join("ultraleap-websocket/build-win-vcpkg/Release/Ultraleap-Tracking-WS.exe"),
                args: vec![],
                cwd: root.join("ultraleap-websocket/build-win-vcpkg/Release"),
            },
            "web" => Spec {
                program: PathBuf::from(std::env::var("ComSpec").unwrap_or_else(|_| "C:/Windows/System32/cmd.exe".into())),
                args: vec!["/c".into(), "npm".into(), "run".into(), "dev".into()],
                cwd: root.join("visual-test"),
            },
            "native" => {
                let panel = self.panel().ok_or("panel is not attached to the desktop")?;
                let mut args = vec!["--at".into(), format!("{},{}", panel.x, panel.y)];
                // Present on the GPU that drives the panel's output.
                if panel.adapter.to_lowercase().contains("intel") {
                    args.push("--low-power".into());
                }
                Spec {
                    program: root.join("northstar-native/target/release/northstar-native.exe"),
                    args,
                    cwd: root.join("northstar-native"),
                }
            }
            "kiosk" => {
                let panel = self.panel().ok_or("panel is not attached to the desktop")?;
                let browser = ["C:/Program Files/Google/Chrome/Application/chrome.exe", "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe", "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"]
                    .iter()
                    .map(PathBuf::from)
                    .find(|p| p.exists())
                    .ok_or("no Chrome or Edge found")?;
                Spec {
                    program: browser,
                    args: vec![
                        "--new-window".into(),
                        "--kiosk".into(),
                        // Console and page state readable with Chrome DevTools Protocol (agents and humans alike).
                        "--remote-debugging-port=9222".into(),
                        // The panel hangs off the discrete GPU; rendering there avoids a cross-adapter copy
                        // and the Intel iGPU, which cannot hold 2880x1600 at the panel rate.
                        "--force_high_performance_gpu".into(),
                        "--autoplay-policy=no-user-gesture-required".into(),
                        format!("--window-position={},{}", panel.x, panel.y),
                        format!("--window-size={},{}", panel.width, panel.height),
                        "--user-data-dir=".to_string() + &root.join("logs/kiosk-profile").to_string_lossy(),
                        format!("http://localhost:5173{}", KIOSK_PAGES.iter().find(|(n, _)| *n == self.page.lock().unwrap().as_str()).map_or("/#headset", |(_, p)| p)),
                    ],
                    cwd: root.to_path_buf(),
                }
            }
            _ => return Err(format!("unknown process {name}")),
        })
    }

    fn start(&self, name: &str) -> Result<(), String> {
        // The bridges own the sensors; the apps are their clients, and only one app
        // can have the panel at a time.
        if name == "native" || name == "kiosk" {
            self.procs.stop(if name == "native" { "kiosk" } else { "native" });
            self.start("xr50")?;
            self.start("leap")?;
            if name == "kiosk" && !port_open(5173) {
                self.start("web")?;
                // Vite needs a moment before the kiosk asks for the page.
                for _ in 0..40 {
                    if port_open(5173) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
        self.procs.start(name, self.spec(name)?)
    }

    /// Pushes foreign windows off the panel onto the laptop display.
    fn sweep_windows(&self) -> Vec<String> {
        let displays = display::list();
        let (Some(panel), Some(laptop)) = (displays.iter().find(|d| d.is_panel && d.attached), displays.iter().find(|d| !d.is_panel && d.attached)) else { return vec![] };
        windows::sweep((panel.x, panel.y, panel.width as i32, panel.height as i32), (laptop.x + 80, laptop.y + 80))
    }

    /// Elevated only: restarts the Ultraleap service when its frames stop and re-plugs the
    /// XR50 when it answers degraded, both of which a USB-C link drop causes.
    fn auto_recover(&self, last: &mut std::time::Instant, last_xr50: &mut std::time::Instant) {
        if last.elapsed() < std::time::Duration::from_secs(45) {
            return;
        }
        let usb = devices::usb();
        let leap = self.procs.tail("leap", 4);
        if usb.leap && self.procs.running("leap") && leap.lines().count() >= 4 && leap.lines().all(|l| l.starts_with("0 frames/s")) {
            log::warn!("Ultraleap frames stopped: restarting the service");
            *last = std::time::Instant::now();
            if let Err(e) = devices::restart_leap_service() {
                log::warn!("leap service: {e}");
            }
            return;
        }
        // The XR50 re-plug helps once per episode at best (a degraded unit usually needs real
        // power), so it runs at most every ten minutes.
        let xr50 = self.procs.tail("xr50", 3);
        if usb.xr50 && self.procs.running("xr50") && (xr50.contains("Degraded open") || xr50.contains("Failed to open device")) && last_xr50.elapsed() > std::time::Duration::from_secs(600) {
            log::warn!("XR50 not answering: re-plugging it over USB");
            *last = std::time::Instant::now();
            *last_xr50 = std::time::Instant::now();
            if let Err(e) = devices::reset_xr50_usb() {
                log::warn!("xr50 reset: {e}");
            }
        }
    }

    fn status(&self) -> Value {
        let displays = display::list();
        let panel = displays.iter().find(|d| d.is_panel).cloned();
        let usb = devices::usb();
        let processes: serde_json::Map<String, Value> = ["xr50", "leap", "web", "native", "kiosk"]
            .iter()
            .map(|n| (n.to_string(), Value::Bool(self.procs.running(n) || (*n == "web" && port_open(5173)))))
            .collect();
        let logs: serde_json::Map<String, Value> = ["xr50", "leap", "native", "web"]
            .iter()
            .map(|n| (n.to_string(), Value::String(self.procs.tail(n, 4))))
            .collect();
        json!({
            "displays": displays,
            "panel": panel.as_ref().map(|p| json!({
                "attached": p.attached, "primary": p.primary, "x": p.x, "y": p.y, "rotation": p.orientation * 90,
                "width": p.width, "height": p.height, "hz": p.hz,
                "native": display::native_mode(p),
            })),
            "layoutOk": display::layout_ok(&displays),
            "usb": usb,
            "leapService": devices::leap_service_state(),
            "elevated": windows::elevated(),
            "processes": processes,
            "kioskPage": *self.page.lock().unwrap(),
            "xr50Source": *self.xr50_source.lock().unwrap(),
            "map": if *self.xr50_source.lock().unwrap() == "sdk" {
                bridge_get("/map/status").and_then(|b| serde_json::from_str::<Value>(&b).ok())
            } else {
                None
            },
            "kioskPages": KIOSK_PAGES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            "dev": {
                "native": self.dev.enabled.load(Ordering::Relaxed),
                "building": self.dev.building.load(Ordering::Relaxed),
                "lastResult": *self.dev.last_result.lock().unwrap(),
            },
            "logs": logs,
        })
    }

    fn action(&self, path: &str) -> Result<Value, String> {
        let parts: Vec<&str> = path.trim_start_matches("/api/").split('/').collect();
        match parts.as_slice() {
            ["display", "fix"] => display::fix_layout().map(|m| json!({ "ok": true, "message": m })),
            ["display", "sweep"] => {
                let moved = self.sweep_windows();
                Ok(json!({ "ok": true, "message": if moved.is_empty() { "nothing on the panel".to_string() } else { format!("moved: {}", moved.join(", ")) } }))
            }
            ["leap-service", "restart"] => devices::restart_leap_service().map(|_| json!({ "ok": true })),
            ["xr50", "reset-usb"] => devices::reset_xr50_usb().map(|_| json!({ "ok": true })),
            ["all", "start"] => {
                for name in ["xr50", "leap", "web"] {
                    self.start(name)?;
                }
                Ok(json!({ "ok": true }))
            }
            ["all", "stop"] => {
                self.procs.stop_all();
                Ok(json!({ "ok": true }))
            }
            ["map", action @ ("save" | "load")] => {
                let body = bridge_get(&format!("/map/{action}")).ok_or("the XR50 bridge is not running")?;
                serde_json::from_str(&body).map_err(|e| e.to_string())
            }
            ["xr50", "source", source @ ("edge" | "recon" | "sdk")] => {
                *self.xr50_source.lock().unwrap() = source.to_string();
                // Clients reconnect on their own, so a running bridge just restarts in the new mode.
                if self.procs.running("xr50") {
                    self.procs.stop("xr50");
                    self.start("xr50")?;
                }
                Ok(json!({ "ok": true, "message": format!("XR50 source: {source}") }))
            }
            ["kiosk", "page", page] if KIOSK_PAGES.iter().any(|(n, _)| n == page) => {
                *self.page.lock().unwrap() = page.to_string();
                Ok(json!({ "ok": true, "message": format!("WebXR page: {page}") }))
            }
            ["dev", "native", on] => {
                self.dev.enabled.store(*on == "on", Ordering::Relaxed);
                Ok(json!({ "ok": true }))
            }
            [name, "start"] => self.start(name).map(|_| json!({ "ok": true })),
            [name, "stop"] => {
                self.procs.stop(name);
                Ok(json!({ "ok": true }))
            }
            _ => Err(format!("unknown action {path}")),
        }
    }
}

/// GET `path` on the XR50 bridge (localhost:8080); the JSON body, or None when it is not up.
fn bridge_get(path: &str) -> Option<String> {
    use std::io::{Read, Write};
    let timeout = std::time::Duration::from_millis(500);
    let mut s = std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], 8080)), timeout).ok()?;
    s.set_read_timeout(Some(timeout)).ok()?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").ok()?;
    let mut response = String::new();
    s.read_to_string(&mut response).ok()?;
    response.split_once("\r\n\r\n").map(|(_, body)| body.to_string())
}

/// True when something answers on localhost:`port` (the Vite server may be run outside the hub).
fn port_open(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], port)), std::time::Duration::from_millis(300)).is_ok()
}

fn reply(request: Request, status: u16, body: String, content_type: &str) {
    let header = Header::from_bytes("Content-Type", content_type).unwrap();
    let _ = request.respond(Response::from_string(body).with_status_code(status).with_header(header));
}

fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("VIBESTAR_ROOT") {
        return PathBuf::from(root);
    }
    let exe = std::env::current_exe().expect("exe path");
    // <root>/northstar-hub/target/release/northstar-hub.exe
    exe.ancestors().nth(4).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let auto_display = !std::env::args().any(|a| a == "--no-auto-display");
    let root = repo_root();
    log::info!("repo root: {}", root.display());

    let hub = Arc::new(Hub { procs: Processes::new(root.clone()), dev: Arc::new(watch::DevState::default()), page: Mutex::new("headset".into()), xr50_source: Mutex::new(std::env::var("NS_XR50_SOURCE").unwrap_or_else(|_| "edge".into())) });

    // Native hot reload: rebuild on source change, then relaunch on the headset.
    let relaunch = hub.clone();
    watch::spawn(hub.dev.clone(), root.join("northstar-native"), move || {
        relaunch.procs.stop("native");
        if let Err(e) = relaunch.start("native") {
            log::warn!("relaunch: {e}");
        }
    });

    // Keep the desktop sane whenever the panel (re)appears, and put the running app back on it:
    // a USB-C link drop re-enumerates the panel, which loses its rotation and strands the window.
    if auto_display {
        let hub = hub.clone();
        std::thread::spawn(move || {
            let mut was_attached = false;
            loop {
                let displays = display::list();
                let attached = displays.iter().any(|d| d.is_panel && d.attached);
                if attached && !display::layout_ok(&displays) {
                    match display::fix_layout() {
                        Ok(m) => log::info!("display layout fixed: {m}"),
                        Err(e) => log::warn!("display layout: {e}"),
                    }
                }
                if attached && !was_attached {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let moved = hub.sweep_windows();
                    if !moved.is_empty() {
                        log::info!("moved off the panel: {}", moved.join(", "));
                    }
                    for name in ["native", "kiosk"] {
                        if hub.procs.running(name) {
                            log::info!("panel is back: relaunching {name}");
                            hub.procs.stop(name);
                            if let Err(e) = hub.start(name) {
                                log::warn!("relaunch {name}: {e}");
                            }
                        }
                    }
                }
                was_attached = attached;
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
    }

    // Sensor recovery without clicking through UAC, when the hub was started elevated.
    if windows::elevated() {
        let hub = hub.clone();
        std::thread::spawn(move || {
            let mut last = std::time::Instant::now() - std::time::Duration::from_secs(60);
            let mut last_xr50 = std::time::Instant::now() - std::time::Duration::from_secs(600);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                hub.auto_recover(&mut last, &mut last_xr50);
            }
        });
        log::info!("running elevated: automatic Ultraleap / XR50 recovery on");
    } else {
        log::info!("not elevated: sensor recovery needs the buttons (run-elevated.cmd avoids the prompts)");
    }

    let server = Server::http(("127.0.0.1", PORT)).expect("bind hub port");
    println!("NorthStar hub: http://localhost:{PORT}");
    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/").to_string();
        match (request.method(), path.as_str()) {
            (Method::Get, "/") => reply(request, 200, UI.to_string(), "text/html; charset=utf-8"),
            (Method::Get, "/api/status") => reply(request, 200, hub.status().to_string(), "application/json"),
            // Lets a rebuild replace the exe without an elevated kill; children are stopped on drop.
            (Method::Post, "/api/hub/exit") => {
                reply(request, 200, json!({ "ok": true }).to_string(), "application/json");
                break;
            }
            (Method::Post, p) if p.starts_with("/api/") => match hub.action(p) {
                Ok(v) => reply(request, 200, v.to_string(), "application/json"),
                Err(e) => reply(request, 400, json!({ "ok": false, "error": e }).to_string(), "application/json"),
            },
            _ => reply(request, 404, "not found".into(), "text/plain"),
        }
    }
}
