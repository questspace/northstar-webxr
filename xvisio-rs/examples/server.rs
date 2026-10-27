//! Vibestar Server — all-in-one XR50 SLAM → WebSocket → browser.
//!
//! Replaces server.js with zero Node.js dependency:
//!   - Streams 6DOF pose from XR50 via hidapi
//!   - Broadcasts JSON over WebSocket to all connected browsers
//!   - Serves visual-test/dist/ static files on HTTP
//!
//! Usage:
//!   cargo run --release --example server            # edge SLAM
//!   cargo run --release --example server -- --sdk   # official SDK: camera SLAM, planes, maps
//!   cargo run --release --features camera --example server -- --recon
//!       # edge SLAM + our own room map: mesh blocks (binary messages, `xvisio::recon::map`
//!       # wire format) and planes (same JSON as the SDK source)
//!   Open http://localhost:8080

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::Message;

const PORT: u16 = 8080;

/// WebSocket clients and whether each asked for every sample (`?rate=full`) rather
/// than the display-rate throttle.
type Clients = Arc<Mutex<Vec<(Arc<Mutex<tungstenite::WebSocket<TcpStream>>>, bool)>>>;

fn main() {
    env_logger::init();

    let dist_dir = find_dist_dir();
    eprintln!("[HTTP] Serving static files from: {}", dist_dir.display());

    let clients: Clients = Arc::new(Mutex::new(Vec::new()));

    // Start XR50 SLAM thread
    let slam_clients = clients.clone();
    let slam_running = Arc::new(AtomicBool::new(true));
    let slam_stop = slam_running.clone();

    // Source: `--sdk` / `--recon` or XR50_SOURCE=sdk|recon; default on-device edge SLAM.
    let source = |name: &str| {
        std::env::args().any(|a| a == format!("--{name}")) || std::env::var("XR50_SOURCE").is_ok_and(|v| v.eq_ignore_ascii_case(name))
    };
    let (sdk, recon) = (source("sdk"), source("recon"));
    let poses = recon.then(xvisio::recon::PoseHistory::default);
    if let Some(poses) = &poses {
        start_mapping(clients.clone(), poses.clone(), slam_running.clone());
    }
    let slam_thread = std::thread::Builder::new()
        .name("xr50-slam".into())
        .spawn(move || {
            if sdk {
                sdk_loop(slam_clients, slam_stop)
            } else {
                slam_loop(slam_clients, slam_stop, poses)
            }
        })
        .expect("Failed to spawn SLAM thread");

    // TCP listener for both HTTP and WebSocket
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT)).unwrap_or_else(|e| {
        eprintln!("Failed to bind port {}: {}", PORT, e);
        std::process::exit(1);
    });

    eprintln!();
    eprintln!("  ╔══════════════════════════════════════╗");
    eprintln!("  ║          VIBESTAR  SERVER             ║");
    eprintln!("  ╠══════════════════════════════════════╣");
    eprintln!("  ║  http://localhost:{}              ║", PORT);
    eprintln!("  ╚══════════════════════════════════════╝");
    eprintln!();

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[TCP] accept error: {}", e);
                continue;
            }
        };

        let clients = clients.clone();
        let dist_dir = dist_dir.clone();

        std::thread::spawn(move || {
            handle_connection(stream, clients, &dist_dir);
        });
    }

    slam_running.store(false, Ordering::Relaxed);
    let _ = slam_thread.join();
}

/// Route incoming connection to WebSocket or HTTP handler.
fn handle_connection(stream: TcpStream, clients: Clients, dist_dir: &Path) {
    // Set initial timeouts for HTTP; WebSocket handler overrides these
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_nodelay(true).ok();

    // Peek to determine if WebSocket upgrade
    let mut peek_buf = [0u8; 4096];
    let n = match stream.peek(&mut peek_buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let request_str = String::from_utf8_lossy(&peek_buf[..n]);

    if request_str.contains("Upgrade: websocket") || request_str.contains("upgrade: websocket") {
        handle_websocket(stream, clients, request_str.contains("rate=full"));
    } else {
        handle_http(stream, &request_str, dist_dir);
    }
}

/// Handle WebSocket by adding it to the broadcast list.
///
/// The SLAM thread owns writes and removes stale clients on send failure.
/// A read loop here would contend with the broadcaster and stall pose updates.
fn handle_websocket(stream: TcpStream, clients: Clients, full_rate: bool) {
    // Keep dead clients from blocking the SLAM broadcaster.
    stream.set_write_timeout(Some(Duration::from_millis(50))).ok();

    let ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[WS] handshake error: {}", e);
            return;
        }
    };

    let ws = Arc::new(Mutex::new(ws));
    {
        let mut list = clients.lock().unwrap();
        list.push((ws.clone(), full_rate));
        eprintln!("[WS] Client connected ({} total{})", list.len(), if full_rate { ", full rate" } else { "" });
    }
}

/// Serve static files from dist/ over HTTP.
fn handle_http(mut stream: TcpStream, request_str: &str, dist_dir: &Path) {
    // Consume the full HTTP request from the socket (peek didn't consume it)
    let mut request_buf = vec![0u8; 8192];
    let _ = stream.read(&mut request_buf);

    let path = request_str
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let url_path = path.split('?').next().unwrap_or(path);
    if let Some(action) = url_path.strip_prefix("/map/") {
        let body = map_action(action);
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        return;
    }
    let url_path = if url_path == "/" {
        "/index.html"
    } else {
        url_path
    };

    // Resolve file path — try exact match, then SPA fallback
    let file_path = dist_dir.join(url_path.trim_start_matches('/'));
    let (body, resolved_path) = match std::fs::read(&file_path) {
        Ok(b) => (b, file_path),
        Err(_) => {
            let fallback = dist_dir.join("index.html");
            match std::fs::read(&fallback) {
                Ok(b) => (b, fallback),
                Err(_) => {
                    let resp = "HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\nNot found";
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                    return;
                }
            }
        }
    };

    let ext = resolved_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("html");
    let mime = match ext {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "glsl" => "text/plain",
        _ => "application/octet-stream",
    };

    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        mime,
        body.len()
    );

    // Write header
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    // Write body in chunks to avoid send buffer overflow
    for chunk in body.chunks(65536) {
        if stream.write_all(chunk).is_err() {
            return;
        }
    }
    let _ = stream.flush();
}

/// SLAM streaming loop — reads XR50 poses and broadcasts JSON to WebSocket clients.
/// Edge SLAM to every client. With `poses` (the recon source) the cameras stream too and
/// every pose also goes to the mapper.
fn slam_loop(clients: Clients, running: Arc<AtomicBool>, poses: Option<xvisio::recon::PoseHistory>) {
    // A USB re-enumeration (selective suspend, cable) can hand back a device whose
    // control queries return an empty identity / zero features; SLAM from that
    // state streams garbage, so keep reopening until it answers (a re-plug clears it).
    let mut degraded_attempts = 0u32;

    while running.load(Ordering::Relaxed) {
        eprintln!("[XR50] Opening device...");

        let mut device = match xvisio::Device::open_first() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[XR50] Failed to open device: {}", e);
                eprintln!("[XR50] Retrying in 1s...");
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };

        eprintln!("[XR50] UUID:     {}", device.uuid());
        eprintln!("[XR50] Version:  {}", device.version());
        eprintln!("[XR50] Features: {:?}", device.features());

        // A USB re-enumeration (e.g. after power/suspend) can hand back a device
        // whose control queries return an empty identity / zero features. SLAM
        // from that state streams stale poses, so reinitialize before using it.
        if device.uuid().trim().is_empty() || device.features().is_empty() {
            degraded_attempts += 1;
            if degraded_attempts % 5 == 1 {
                eprintln!(
                    "[XR50] Degraded open (empty identity/features) - waiting for the device to recover (attempt {}); re-plug the headset USB if this persists",
                    degraded_attempts
                );
            }
            drop(device);
            std::thread::sleep(Duration::from_millis(1500));
            continue;
        }
        degraded_attempts = 0;

        let started = if poses.is_some() { device.start_slam_with_cameras(xvisio::SlamMode::Edge) } else { device.start_slam(xvisio::SlamMode::Edge) };
        let stream = match started {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[XR50] Failed to start SLAM: {}", e);
                eprintln!("[XR50] Retrying in 1s...");
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };

        eprintln!("[XR50] Streaming SLAM data to WebSocket clients...");

        // Optional pose diagnostics for investigating rotation glitches.
        //   XR50_POSE_LOG=<path>  capture every sample as CSV
        //   XR50_SPIKE_DEG=<deg>  per-sample angular-delta spike threshold (default 20)
        // Enables spike detection on stderr and, if a path is set, a full CSV
        // trace (raw quaternion + emitted euler + confidence + angular delta) so
        // a data spike (large quat delta) can be told apart from an euler
        // representation glitch (small quat delta, large euler jump).
        let mut pose_log = std::env::var("XR50_POSE_LOG").ok().and_then(|p| {
            match std::fs::OpenOptions::new().create(true).append(true).open(&p) {
                Ok(mut f) => {
                    let _ = writeln!(
                        f,
                        "# host_s,dev_us,qx,qy,qz,qw,roll,pitch,yaw,conf,d_ang_deg,spike,det,normErr,ortho"
                    );
                    eprintln!("[XR50] Pose CSV logging to {}", p);
                    Some(f)
                }
                Err(e) => {
                    eprintln!("[XR50] pose log open failed ({}): {}", p, e);
                    None
                }
            }
        });
        let spike_threshold_deg: f64 = std::env::var("XR50_SPIKE_DEG")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20.0);
        let diagnostics = pose_log.is_some() || std::env::var("XR50_SPIKE_DEG").is_ok();
        let mut prev_quat: Option<[f64; 4]> = None;

        let mut count: u64 = 0;
        let mut ws_sent: u64 = 0;
        let mut last_report = std::time::Instant::now();
        let mut last_broadcast = std::time::Instant::now();
        let mut last_sample = std::time::Instant::now();
        // Matches the NorthStar panel's 120 Hz; the native pipeline showed each
        // 8 ms of throttle shows up one-to-one as pose age at the warp pass.
        let broadcast_interval = Duration::from_millis(8);
        // Edge-SLAM stops emitting poses when it loses its lock (e.g. the headset
        // sits idle with no visual parallax). Reinitialize instead of spinning on
        // timeouts forever, so the stream recovers on its own once moved again.
        let stall_timeout = Duration::from_secs(4);

        while running.load(Ordering::Relaxed) {
            let sample = match stream.recv_timeout(Duration::from_millis(500)) {
                Ok(s) => s,
                Err(xvisio::XvisioError::Timeout) => {
                    if last_sample.elapsed() >= stall_timeout {
                        eprintln!(
                            "[XR50] No pose for {}s - SLAM stalled, reinitializing...",
                            stall_timeout.as_secs()
                        );
                        break;
                    }
                    continue;
                }
                Err(e) => {
                    eprintln!("[XR50] Stream stopped: {}", e);
                    break;
                }
            };

            last_sample = std::time::Instant::now();
            count += 1;
            if let Some(poses) = &poses {
                poses.push(sample.pose);
            }
            let now = std::time::Instant::now();

            // Per-sample rotation diagnostics (runs at full sample rate so a
            // single-frame spike can't be hidden by the 60 Hz broadcast throttle).
            if diagnostics {
                let p = &sample.pose;
                let q = p.quaternion;
                let d_ang = prev_quat.map(|prev| quat_angle_deg(prev, q)).unwrap_or(0.0);
                prev_quat = Some(q);
                let spike = d_ang >= spike_threshold_deg;
                if let Some(f) = pose_log.as_mut() {
                    let e = p.euler_deg;
                    let (det, norm_err, ortho) = matrix_validity(&p.rotation);
                    let _ = writeln!(
                        f,
                        "{:.6},{},{:.5},{:.5},{:.5},{:.5},{:.2},{:.2},{:.2},{:.4},{:.2},{},{:.4},{:.4},{:.4}",
                        p.host_timestamp_s, p.timestamp_us,
                        q[0], q[1], q[2], q[3],
                        e[0], e[1], e[2],
                        p.confidence, d_ang, spike as u8,
                        det, norm_err, ortho,
                    );
                }
                if spike {
                    let e = p.euler_deg;
                    eprintln!(
                        "[XR50] SPIKE d{:.1}deg conf={:.3} euler=({:.1},{:.1},{:.1}) quat=({:.3},{:.3},{:.3},{:.3})",
                        d_ang, p.confidence, e[0], e[1], e[2], q[0], q[1], q[2], q[3],
                    );
                }
            }

            // Display-rate throttle for browsers; full-rate clients get every sample.
            let tick = now.duration_since(last_broadcast) >= broadcast_interval;
            let any_full = clients.lock().unwrap().iter().any(|(_, full)| *full);
            if tick || any_full {
                if tick {
                    last_broadcast = now;
                }

                let p = &sample.pose;
                let json = pose_json(p.translation, p.quaternion, p.confidence, p.host_timestamp_s, p.timestamp_us as i64);
                ws_sent += send(&clients, json, tick);
            }

            // Report throughput every 5 seconds
            if now.duration_since(last_report) >= Duration::from_secs(5) {
                let clients_count = clients.lock().unwrap().len();
                let elapsed = last_report.elapsed().as_secs_f64();
                eprintln!(
                    "[XR50] {} samples/s, {} client-msg/s, {} client(s)",
                    (count as f64 / elapsed) as u32,
                    (ws_sent as f64 / elapsed) as u32,
                    clients_count
                );
                count = 0;
                ws_sent = 0;
                last_report = now;
            }
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}

/// One pose message. Raw XR50 convention (world and device in camera convention, meters,
/// `[qx,qy,qz,qw]`); frame conversion is the client's job, Euler stays for the HUD/legacy
/// readers. "tw" is wall-clock ms at broadcast so a client on the same machine can measure
/// pose age against Date.now() (same metric as the native pipeline).
fn pose_json(t: [f64; 3], q: [f64; 4], confidence: f64, host_s: f64, device_us: i64) -> String {
    let wall_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1e3)
        .unwrap_or(0.0);
    let e = xvisio::protocol::quaternion_to_euler(q[3], q[0], q[1], q[2]);
    format!(
        "{{\"x\":{:.4},\"y\":{:.4},\"z\":{:.4},\"q\":[{:.5},{:.5},{:.5},{:.5}],\"c\":{:.2},\"tw\":{:.3},\"roll\":{:.1},\"pitch\":{:.1},\"yaw\":{:.1},\"t\":{:.6},\"tu\":{}}}",
        t[0], t[1], t[2], q[0], q[1], q[2], q[3], confidence, wall_ms, e[0], e[1], e[2], host_s, device_us,
    )
}

/// Sends `msg` (JSON text or binary) to full-rate clients, and to everyone when `tick` (the
/// display-rate throttle) is due. Drops clients whose socket failed. Returns the number of sends.
fn send(clients: &Clients, msg: impl Into<Message>, tick: bool) -> u64 {
    let msg: Message = msg.into();
    let mut sent = 0u64;
    clients.lock().unwrap().retain(|(ws_arc, full)| {
        if !(*full || tick) {
            return true;
        }
        let ok = ws_arc.lock().unwrap().send(msg.clone()).is_ok();
        sent += ok as u64;
        ok
    });
    sent
}

/// The SDK helper's stdin (map commands go there) and its latest map event.
static HELPER_STDIN: Mutex<Option<std::process::ChildStdin>> = Mutex::new(None);
static MAP_STATUS: Mutex<String> = Mutex::new(String::new());

/// The room map: saved on request, loaded automatically whenever the SDK source starts.
fn map_path() -> PathBuf {
    std::env::var("XR50_MAP").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../maps/room.xvmap")
    })
}

/// `/map/save`, `/map/load`, `/map/status` (SDK source only). Save and load answer at once;
/// the result arrives as a map event (`/map/status`, and broadcast to clients).
fn map_action(action: &str) -> String {
    let status = || MAP_STATUS.lock().unwrap().clone();
    let path = map_path();
    let command = match action {
        "status" => {
            let s = status();
            return format!("{{\"ok\":true,\"saved\":{},\"last\":{}}}", path.exists(), if s.is_empty() { "null".into() } else { s });
        }
        "save" => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            "save"
        }
        "load" => "load",
        _ => return "{\"ok\":false,\"error\":\"unknown map action\"}".into(),
    };
    let mut stdin = HELPER_STDIN.lock().unwrap();
    let Some(pipe) = stdin.as_mut() else {
        return "{\"ok\":false,\"error\":\"maps need the SDK source (server --sdk)\"}".into();
    };
    match writeln!(pipe, "{command} {}", path.display()).and_then(|_| pipe.flush()) {
        Ok(()) => format!("{{\"ok\":true,\"message\":\"{command} requested\"}}"),
        Err(e) => format!("{{\"ok\":false,\"error\":\"{e}\"}}"),
    }
}

/// Pose source: the official XVisio SDK through `xvsdk-bridge` (Windows), which runs SLAM on
/// the host from the fisheye cameras + IMU and detects planes. The helper prints JSON lines;
/// poses are converted to the raw XR50 convention and broadcast like edge poses, plane
/// updates (`{"type":"planes",...}`, SDK world = raw XR50 world axes) go to every client.
///
/// Frame conversion, measured on the head on 2026-10-26 (level / tilted down / turned left,
/// gravity from the IMU): the SDK world is gravity-aligned with Y down like the raw world
/// (only the starting yaw differs), and the SDK reports the IMU frame, which is the XR50
/// camera frame rotated 180° about Z. So q_raw = q_sdk · Rz(180°), translation unchanged.
fn sdk_loop(clients: Clients, running: Arc<AtomicBool>) {
    use std::io::BufRead;
    let exe = std::env::var("XVSDK_BRIDGE").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools/xvsdk/sdk/bin/xvsdk-bridge.exe")
    });
    // Rz(180°) on the right: q · [0,0,1,0]
    let to_raw = |q: [f64; 4]| [q[1], -q[0], q[3], -q[2]];
    let num = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);

    while running.load(Ordering::Relaxed) {
        eprintln!("[XR50/SDK] starting {}", exe.display());
        let child = std::process::Command::new(&exe)
            .args(["--mode", "mixed", "--exit-on-stdin-eof", "--map"])
            .arg(map_path())
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[XR50/SDK] cannot start the helper: {e} (build it with xvsdk-bridge\\build.ps1)");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        *HELPER_STDIN.lock().unwrap() = child.stdin.take();
        let (mut count, mut ws_sent, mut planes) = (0u64, 0u64, 0usize);
        let mut last_broadcast = std::time::Instant::now();
        let mut last_report = std::time::Instant::now();
        let mut last_planes = std::time::Instant::now() - Duration::from_secs(1);
        let broadcast_interval = Duration::from_millis(8);

        // The SDK also logs to stdout: only JSON lines are data.
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if !running.load(Ordering::Relaxed) {
                break;
            }
            if !line.starts_with('{') {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            if v.get("type").and_then(|t| t.as_str()) == Some("map") {
                eprintln!("[XR50/SDK] map: {line}");
                *MAP_STATUS.lock().unwrap() = line.clone();
                ws_sent += send(&clients, line, true);
                continue;
            }
            if v.get("type").and_then(|t| t.as_str()) == Some("planes") {
                // The SDK refines planes ~50 times a second; clients need the latest set, not each step.
                planes = v["planes"].as_array().map_or(0, |a| a.len());
                if last_planes.elapsed() >= Duration::from_millis(200) {
                    last_planes = std::time::Instant::now();
                    ws_sent += send(&clients, line, true);
                }
                continue;
            }
            let Some(q) = v.get("q").and_then(|q| q.as_array()) else { continue };
            let q: Vec<f64> = q.iter().filter_map(|x| x.as_f64()).collect();
            if q.len() != 4 {
                continue;
            }
            count += 1;
            let now = std::time::Instant::now();
            let tick = now.duration_since(last_broadcast) >= broadcast_interval;
            let any_full = clients.lock().unwrap().iter().any(|(_, full)| *full);
            if tick || any_full {
                if tick {
                    last_broadcast = now;
                }
                let json = pose_json(
                    [num(&v, "x"), num(&v, "y"), num(&v, "z")],
                    to_raw([q[0], q[1], q[2], q[3]]),
                    num(&v, "c"),
                    num(&v, "t"),
                    num(&v, "tu") as i64,
                );
                ws_sent += send(&clients, json, tick);
            }
            if now.duration_since(last_report) >= Duration::from_secs(5) {
                let elapsed = last_report.elapsed().as_secs_f64();
                eprintln!(
                    "[XR50/SDK] {} poses/s, {} client-msg/s, {} plane(s), {} client(s)",
                    (count as f64 / elapsed) as u32,
                    (ws_sent as f64 / elapsed) as u32,
                    planes,
                    clients.lock().unwrap().len()
                );
                count = 0;
                ws_sent = 0;
                last_report = now;
            }
        }
        HELPER_STDIN.lock().unwrap().take();
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("[XR50/SDK] helper exited - restarting in 2 s");
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Angular difference (degrees) between two quaternions [qx, qy, qz, qw].
/// Normalizes both first so a non-unit wire quaternion can't inflate the angle.
fn quat_angle_deg(a: [f64; 4], b: [f64; 4]) -> f64 {
    let na = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2] + a[3] * a[3]).sqrt();
    let nb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2] + b[3] * b[3]).sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    let dot = ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]) / (na * nb))
        .abs()
        .clamp(0.0, 1.0);
    (2.0 * dot.acos()).to_degrees()
}

/// Rotation-matrix validity metrics: (determinant, max row-norm error, max
/// off-diagonal row dot). A true rotation matrix has det≈+1, row norms≈1
/// (error≈0), and orthogonal rows (dots≈0).
fn matrix_validity(m: &[[f64; 3]; 3]) -> (f64, f64, f64) {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let row_norm = |r: usize| (m[r][0] * m[r][0] + m[r][1] * m[r][1] + m[r][2] * m[r][2]).sqrt();
    let dot = |a: usize, b: usize| m[a][0] * m[b][0] + m[a][1] * m[b][1] + m[a][2] * m[b][2];
    let norm_err = [row_norm(0), row_norm(1), row_norm(2)]
        .iter()
        .map(|n| (n - 1.0).abs())
        .fold(0.0_f64, f64::max);
    let ortho = [dot(0, 1), dot(0, 2), dot(1, 2)]
        .iter()
        .map(|d| d.abs())
        .fold(0.0_f64, f64::max);
    (det, norm_err, ortho)
}

/// Find the visual-test/dist/ directory.
fn find_dist_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("../visual-test/dist"),
        PathBuf::from("visual-test/dist"),
        PathBuf::from("dist"),
    ];

    for p in &candidates {
        if p.join("index.html").exists() {
            return p.canonicalize().unwrap_or_else(|_| p.clone());
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let from_exe = exe_dir.join("../../../../visual-test/dist");
            if from_exe.join("index.html").exists() {
                return from_exe.canonicalize().unwrap_or(from_exe);
            }
        }
    }

    eprintln!("[WARN] visual-test/dist/ not found. HTTP will return 404.");
    eprintln!("       Run from the xvisio-rs/ or vibestar/ directory.");
    PathBuf::from("../visual-test/dist")
}

/// Depth frames fused per second at most (the camera delivers ~45).
#[cfg(feature = "camera")]
const MAP_INTERVAL: Duration = Duration::from_millis(100);

/// Our own room map (recon source): fuses stereo depth at the edge-SLAM poses, then once a
/// second sends the changed mesh blocks (binary) and the planes (`{"type":"planes",...}`, raw
/// XR50 world like the SDK source). A newly connected client gets every block.
#[cfg(feature = "camera")]
fn start_mapping(clients: Clients, poses: xvisio::recon::PoseHistory, running: Arc<AtomicBool>) {
    use xvisio::recon::{detect_planes, RoomMap, TsdfConfig, FRAME_DELAY};
    use xvisio::stereo::{Calibration, StereoConfig, StereoMatcher};
    let path = std::env::var("XR50_CALIBRATION").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../calibration/xr50-fisheye.json")
    });
    let calibration = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Calibration::from_json(&t)) {
        Ok(c) => c,
        Err(e) => {
            return eprintln!("[MAP] no camera calibration at {} ({e}): run xvsdk-bridge --dump-calibration", path.display());
        }
    };
    let matcher = StereoMatcher::new(&calibration, (640, 400), StereoConfig::default());
    std::thread::Builder::new()
        .name("xr50-map".into())
        .spawn(move || {
            // Mapping shares the laptop with the browser and the native app: half the cores and
            // at most MAP_RATE depth frames a second (full speed starved the WebXR page).
            let threads = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1));
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("map thread pool");
            let mut map = RoomMap::new(TsdfConfig::default());
            let (mut last_update, mut last_fused, mut known_clients) = (std::time::Instant::now(), std::time::Instant::now(), 0);
            while running.load(Ordering::Relaxed) {
                // The cameras start with SLAM; until then (and after a re-plug) keep retrying.
                let mut camera = match xvisio::camera::StereoCamera::open() {
                    Ok(c) => c,
                    Err(_) => {
                        std::thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                };
                eprintln!("[MAP] cameras streaming, mapping");
                while let Ok(f) = camera.frame() {
                    if last_fused.elapsed() < MAP_INTERVAL {
                        continue;
                    }
                    if let Some(pose) = poses.at(f.received - FRAME_DELAY).filter(|p| p.confidence >= 0.5) {
                        last_fused = std::time::Instant::now();
                        pool.install(|| map.integrate_stereo(&matcher, [&f.images[0], &f.images[1]], &pose));
                    }
                    if last_update.elapsed() < Duration::from_secs(1) {
                        continue;
                    }
                    last_update = std::time::Instant::now();
                    let changed = pool.install(|| map.remesh());
                    let count = clients.lock().unwrap().len();
                    let blocks = if count > known_clients { map.keys() } else { changed };
                    known_clients = count;
                    if !blocks.is_empty() {
                        send(&clients, map.encode(&blocks), true);
                    }
                    send(&clients, planes_json(&detect_planes(&map.mesh(), Default::default())), true);
                }
                eprintln!("[MAP] camera stream ended; reopening");
            }
        })
        .expect("map thread");
}

#[cfg(not(feature = "camera"))]
fn start_mapping(_: Clients, _: xvisio::recon::PoseHistory, _: Arc<AtomicBool>) {
    eprintln!("[MAP] the recon source needs the camera feature: cargo build --release --features camera --example server");
}

/// Planes in the SDK source's message format: id, unit normal, offset, border polygon.
#[cfg(feature = "camera")]
fn planes_json(planes: &[xvisio::recon::Plane]) -> String {
    let v3 = |p: &[f32; 3]| format!("[{:.3},{:.3},{:.3}]", p[0], p[1], p[2]);
    let items: Vec<String> = planes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let border: Vec<String> = p.border.iter().map(v3).collect();
            format!("{{\"id\":\"recon-{i}\",\"n\":{},\"d\":{:.3},\"p\":[{}]}}", v3(&p.normal), p.offset, border.join(","))
        })
        .collect();
    format!("{{\"type\":\"planes\",\"planes\":[{}]}}", items.join(","))
}
