//! Streams the XR50's full-rate IMU (`Device::start_imu`): accelerometer (m/s²), gyroscope
//! (rad/s) and magnetometer, one line per 100 samples, then the rate.
//!
//!   cargo run --release --example imu_probe -- [seconds]
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let mut device = xvisio::Device::open_first()?;
    let stream = device.start_imu()?;

    let (start, mut count) = (Instant::now(), 0u64);
    while start.elapsed() < Duration::from_secs(seconds) {
        let Ok(imu) = stream.recv_imu_timeout(Duration::from_millis(500)) else { continue };
        count += 1;
        if count % 100 == 1 {
            let (a, g, m) = (imu.accelerometer, imu.gyroscope, imu.magnetometer);
            println!(
                "ts={:<10} accel=[{:+.3}, {:+.3}, {:+.3}] m/s2  gyro=[{:+.4}, {:+.4}, {:+.4}] rad/s  mag=[{:+.1}, {:+.1}, {:+.1}]",
                imu.timestamp_us, a[0], a[1], a[2], g[0], g[1], g[2], m[0], m[1], m[2],
            );
        }
    }
    println!("{count} IMU samples ({:.0} Hz)", count as f64 / start.elapsed().as_secs_f64());
    Ok(())
}
