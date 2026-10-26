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
                args: vec![],
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
            "processes": processes,
            "kioskPage": *self.page.lock().unwrap(),
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

    let hub = Arc::new(Hub { procs: Processes::new(root.clone()), dev: Arc::new(watch::DevState::default()), page: Mutex::new("headset".into()) });

    // Native hot reload: rebuild on source change, then relaunch on the headset.
    let relaunch = hub.clone();
    watch::spawn(hub.dev.clone(), root.join("northstar-native"), move || {
        relaunch.procs.stop("native");
        if let Err(e) = relaunch.start("native") {
            log::warn!("relaunch: {e}");
        }
    });

    // Keep the desktop sane whenever the panel (re)appears.
    if auto_display {
        std::thread::spawn(|| loop {
            let displays = display::list();
            if displays.iter().any(|d| d.is_panel) && !display::layout_ok(&displays) {
                match display::fix_layout() {
                    Ok(m) => log::info!("display layout fixed: {m}"),
                    Err(e) => log::warn!("display layout: {e}"),
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        });
    }

    let server = Server::http(("127.0.0.1", PORT)).expect("bind hub port");
    println!("NorthStar hub: http://localhost:{PORT}");
    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/").to_string();
        match (request.method(), path.as_str()) {
            (Method::Get, "/") => reply(request, 200, UI.to_string(), "text/html; charset=utf-8"),
            (Method::Get, "/api/status") => reply(request, 200, hub.status().to_string(), "application/json"),
            (Method::Post, p) if p.starts_with("/api/") => match hub.action(p) {
                Ok(v) => reply(request, 200, v.to_string(), "application/json"),
                Err(e) => reply(request, 400, json!({ "ok": false, "error": e }).to_string(), "application/json"),
            },
            _ => reply(request, 404, "not found".into(), "text/plain"),
        }
    }
}
