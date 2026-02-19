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

fn main() {
    env_logger::init();

    let dist_dir = find_dist_dir();
    eprintln!("[HTTP] Serving static files from: {}", dist_dir.display());

    // WebSocket clients shared across threads
    type WsClient = Arc<Mutex<tungstenite::WebSocket<TcpStream>>>;
    let clients: Arc<Mutex<Vec<WsClient>>> = Arc::new(Mutex::new(Vec::new()));

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
fn handle_connection(
    stream: TcpStream,
    clients: Arc<Mutex<Vec<Arc<Mutex<tungstenite::WebSocket<TcpStream>>>>>>,
    dist_dir: &Path,
) {
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
        handle_websocket(stream, clients);
    } else {
        handle_http(stream, &request_str, dist_dir);
    }
}

/// Handle WebSocket — add to broadcast list, wait for disconnect.
///
/// The SLAM thread is the sole writer to the WebSocket (no mutex contention).
/// This thread just stays alive and detects when the client is removed from
/// the broadcast list (due to send failure in the SLAM thread).
fn handle_websocket(
    stream: TcpStream,
    clients: Arc<Mutex<Vec<Arc<Mutex<tungstenite::WebSocket<TcpStream>>>>>>,
) {
    // Write timeout prevents the SLAM thread from blocking on a slow client
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

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
        list.push(ws.clone());
        eprintln!("[WS] Client connected ({} total)", list.len());
    }

    // Wait until the SLAM thread removes us from the client list (send failure)
    // or the TCP connection drops.
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let still_active = clients.lock().unwrap().iter().any(|c| Arc::ptr_eq(c, &ws));
        if !still_active {
            break;
        }
    }

    eprintln!(
        "[WS] Client disconnected ({} total)",
        clients.lock().unwrap().len()
    );
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
fn slam_loop(
    clients: Arc<Mutex<Vec<Arc<Mutex<tungstenite::WebSocket<TcpStream>>>>>>,
    running: Arc<AtomicBool>,
) {
    eprintln!("[XR50] Opening device...");

    let mut device = match xvisio::Device::open_first() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[XR50] Failed to open device: {}", e);
            eprintln!("[XR50] Server will continue without tracking data.");
            eprintln!("[XR50] Plug in the XR50 and restart the server.");
            while running.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
            }
            return;
        }
    };

    eprintln!("[XR50] UUID:     {}", device.uuid());
    eprintln!("[XR50] Version:  {}", device.version());
    eprintln!("[XR50] Features: {:?}", device.features());

    let stream = match device.start_slam(xvisio::SlamMode::Edge) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[XR50] Failed to start SLAM: {}", e);
            return;
        }
    };

    eprintln!("[XR50] Streaming SLAM data to WebSocket clients...");

    let mut count: u64 = 0;
    let mut ws_sent: u64 = 0;
    let mut last_report = std::time::Instant::now();
    let mut last_broadcast = std::time::Instant::now();
    let broadcast_interval = Duration::from_millis(16); // ~60 Hz to browser

    while running.load(Ordering::Relaxed) {
        let sample = match stream.recv_timeout(Duration::from_secs(2)) {
            Ok(s) => s,
            Err(xvisio::XvisioError::Timeout) => continue,
            Err(e) => {
                eprintln!("[XR50] Error: {}", e);
                break;
            }
        };

        count += 1;
        let now = std::time::Instant::now();

        // Throttle WebSocket broadcast to ~60 Hz (browser can't use more)
        if now.duration_since(last_broadcast) >= broadcast_interval {
            last_broadcast = now;

            let p = &sample.pose;
            let json = format!(
                "{{\"x\":{:.4},\"y\":{:.4},\"z\":{:.4},\"roll\":{:.1},\"pitch\":{:.1},\"yaw\":{:.1},\"t\":{}}}",
                p.translation[0],
                p.translation[1],
                p.translation[2],
                p.euler_deg[0],
                p.euler_deg[1],
                p.euler_deg[2],
                p.timestamp_us,
            );

            let msg = Message::Text(json);
            let mut list = clients.lock().unwrap();
            list.retain(|ws_arc| {
                let mut ws = ws_arc.lock().unwrap();
                ws.send(msg.clone()).is_ok()
            });
            drop(list);
            ws_sent += 1;
        }

        // Report throughput every 5 seconds
        if now.duration_since(last_report) >= Duration::from_secs(5) {
            let clients_count = clients.lock().unwrap().len();
            let elapsed = last_report.elapsed().as_secs_f64();
            eprintln!(
                "[XR50] {} samples/s, {} ws/s, {} client(s)",
                (count as f64 / elapsed) as u32,
                (ws_sent as f64 / elapsed) as u32,
                clients_count
            );
            count = 0;
            ws_sent = 0;
            last_report = now;
        }
    }
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
