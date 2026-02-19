//! Stream 6DOF SLAM pose data from the XR50 to stdout.
//!
//! Usage: cargo run --example stream
//! Press Ctrl+C to stop.

use std::time::{Duration, Instant};

fn main() {
    env_logger::init();

    let device = match xvisio::Device::open_first() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to open device: {}", e);
            std::process::exit(1);
        }
    };

    println!("UUID:     {}", device.uuid());
    println!("Version:  {}", device.version());
    println!("Features: {:?}", device.features());
    println!();

    let stream = match device.start_slam(xvisio::SlamMode::Edge) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start SLAM: {}", e);
            std::process::exit(1);
        }
    };

    println!("Streaming SLAM (Ctrl+C to stop)...");

    let start = Instant::now();
    let mut count: u64 = 0;
    let mut last_report = Instant::now();

    loop {
        match stream.recv_timeout(Duration::from_secs(2)) {
            Ok(sample) => {
                count += 1;
                let p = &sample.pose;

                // Print every ~100th sample to avoid flooding the terminal
                if count % 100 == 1 {
                    println!(
                        "ts={:<12}  pos=[{:+.4}, {:+.4}, {:+.4}]  quat=[{:+.3}, {:+.3}, {:+.3}, {:+.3}]  conf={:.3}",
                        p.timestamp_us,
                        p.translation[0], p.translation[1], p.translation[2],
                        p.quaternion[0], p.quaternion[1], p.quaternion[2], p.quaternion[3],
                        p.confidence,
                    );
                }

                // Report rate every 3 seconds
                let now = Instant::now();
                if now.duration_since(last_report) >= Duration::from_secs(3) {
                    let elapsed = start.elapsed().as_secs_f64();
                    let hz = count as f64 / elapsed;
                    println!("--- {} samples in {:.1}s ({:.1} Hz) ---", count, elapsed, hz);
                    last_report = now;
                }
            }
            Err(xvisio::XvisioError::Timeout) => {
                eprintln!("Timeout waiting for SLAM data");
                break;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "\nTotal: {} samples in {:.1}s ({:.1} Hz)",
        count,
        elapsed,
        count as f64 / elapsed
    );
}
