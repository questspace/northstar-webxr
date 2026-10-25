//! Vibestar Server — all-in-one XR50 SLAM → WebSocket → browser.
//!
//! Replaces server.js with zero Node.js dependency:
//!   - Streams 6DOF pose from XR50 via hidapi
//!   - Broadcasts JSON over WebSocket to all connected browsers
//!   - Serves visual-test/dist/ static files on HTTP
//!
//! Usage:
//!   cargo run --release --example server
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

    let slam_thread = std::thread::Builder::new()
        .name("xr50-slam".into())
        .spawn(move || {
            slam_loop(slam_clients, slam_stop);
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
fn slam_loop(clients: Clients, running: Arc<AtomicBool>) {
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

        let stream = match device.start_slam(xvisio::SlamMode::Edge) {
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

                // Raw XR50 pose in the device's own world frame (meters, [qx,qy,qz,qw]).
                // Frame conversion is the client's job; Euler stays for the HUD/legacy readers.
                // "tw" is wall-clock ms at broadcast so a browser on the same machine can
                // measure pose age against Date.now() (same metric as the native pipeline).
                let p = &sample.pose;
                let q = &p.quaternion;
                let wall_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64() * 1e3)
                    .unwrap_or(0.0);
                let json = format!(
                    "{{\"x\":{:.4},\"y\":{:.4},\"z\":{:.4},\"q\":[{:.5},{:.5},{:.5},{:.5}],\"c\":{:.2},\"tw\":{:.3},\"roll\":{:.1},\"pitch\":{:.1},\"yaw\":{:.1},\"t\":{:.6},\"tu\":{}}}",
                    p.translation[0],
                    p.translation[1],
                    p.translation[2],
                    q[0], q[1], q[2], q[3],
                    p.confidence,
                    wall_ms,
                    p.euler_deg[0],
                    p.euler_deg[1],
                    p.euler_deg[2],
                    p.host_timestamp_s,
                    p.timestamp_us,
                );

                let msg = Message::Text(json);
                let mut sent = 0u64;
                let mut list = clients.lock().unwrap();
                list.retain(|(ws_arc, full)| {
                    if !(*full || tick) {
                        return true;
                    }
                    let mut ws = ws_arc.lock().unwrap();
                    if ws.send(msg.clone()).is_ok() {
                        sent += 1;
                        true
                    } else {
                        false
                    }
                });
                drop(list);
                ws_sent += sent;
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
