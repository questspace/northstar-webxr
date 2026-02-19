//! Stream 6DOF SLAM pose data as JSON lines for visual-test integration.
//!
//! Outputs one JSON object per line matching the format expected by
//! visual-test/server.js → useXR50.ts:
//!
//! {"x":0.021,"y":0.002,"z":0.028,"roll":5.2,"pitch":3.1,"yaw":1.4,"t":1596314}
//!
//! Usage: cargo run --release --example stream_json

use std::io::{self, Write};
use std::time::Duration;

fn main() {
    env_logger::init();

    let mut device = match xvisio::Device::open_first() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to open device: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("UUID:     {}", device.uuid());
    eprintln!("Version:  {}", device.version());
    eprintln!("Features: {:?}", device.features());

    let stream = match device.start_slam(xvisio::SlamMode::Edge) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start SLAM: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("Streaming JSON (Ctrl+C to stop)...");

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut idle_timeouts: u32 = 0;

    loop {
        match stream.recv_timeout(Duration::from_secs(2)) {
            Ok(sample) => {
                idle_timeouts = 0;
                let p = &sample.pose;
                let _ = writeln!(
                    out,
                    "{{\"x\":{:.4},\"y\":{:.4},\"z\":{:.4},\"roll\":{:.1},\"pitch\":{:.1},\"yaw\":{:.1},\"t\":{}}}",
                    p.translation[0],
                    p.translation[1],
                    p.translation[2],
                    p.euler_deg[0],
                    p.euler_deg[1],
                    p.euler_deg[2],
                    p.timestamp_us,
                );
                let _ = out.flush();
            }
            Err(xvisio::XvisioError::Timeout) => {
                idle_timeouts += 1;
                eprintln!("No SLAM packet for 2s (timeout #{})", idle_timeouts);
                if idle_timeouts >= 15 {
                    eprintln!("Stopping after 30s without SLAM packets");
                    break;
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }
}
