//! Edge SLAM plus the stereo fisheye cameras: prints pose and frame rates and writes the
//! last stereo pair as a PGM image (both cameras side by side).
//!
//!   cargo run --release --features camera --example fisheye -- [seconds] [out.pgm]
use std::time::{Duration, Instant};
use xvisio::camera::StereoCamera;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let out = std::env::args().nth(2).unwrap_or_else(|| "stereo.pgm".into());

    let mut device = xvisio::Device::open_first()?;
    let stream = device.start_slam_with_cameras(xvisio::SlamMode::Edge)?;
    let mut camera = StereoCamera::open()?;

    let (start, mut frames, mut poses) = (Instant::now(), 0u32, 0u32);
    let mut last = None;
    while start.elapsed() < Duration::from_secs(seconds) {
        last = Some(camera.frame()?);
        frames += 1;
        while stream.try_recv().is_some() {
            poses += 1;
        }
    }
    let s = start.elapsed().as_secs_f64();
    println!("camera {:.0} fps, SLAM {:.0} Hz", frames as f64 / s, poses as f64 / s);

    // Side by side: each row is left-image row then right-image row.
    let f = last.ok_or("no frame")?;
    let (w, h) = (f.width as usize, f.height as usize);
    let mut pgm = format!("P5\n{} {}\n255\n", 2 * w, h).into_bytes();
    for y in 0..h {
        pgm.extend_from_slice(&f.images[0][y * w..(y + 1) * w]);
        pgm.extend_from_slice(&f.images[1][y * w..(y + 1) * w]);
    }
    std::fs::write(&out, pgm)?;
    println!("wrote {out} ({}x{} per camera)", w, h);
    Ok(())
}
