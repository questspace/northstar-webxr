//! Records a reconstruction dataset for offline tuning: every edge-SLAM pose with its arrival
//! time, and the depth map of each camera frame (u16 millimetres, 0 = unknown) with its arrival
//! time, plus the rectified stereo pair (images/<n>.bin: left then right, 8-bit) so depth can be
//! recomputed offline with other matcher settings. Layout in <dir>: poses.csv, frames.csv,
//! depth/<n>.bin, images/<n>.bin, intrinsics.json.
//!
//!   cargo run --release --features camera --example recon_record -- [seconds] [calibration.json] [dir]
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use xvisio::camera::StereoCamera;
use xvisio::stereo::{Calibration, StereoConfig, StereoMatcher};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let calibration = Calibration::from_json(&std::fs::read_to_string(args.get(2).map(String::as_str).unwrap_or("../calibration/xr50-fisheye.json"))?)?;
    let dir = std::path::PathBuf::from(args.get(3).cloned().unwrap_or_else(|| "recon-dataset".into()));
    std::fs::create_dir_all(dir.join("depth"))?;
    std::fs::create_dir_all(dir.join("images"))?;

    let config = StereoConfig::default();
    let matcher = StereoMatcher::new(&calibration, (640, 400), config);
    let r = matcher.rect_rotation;
    std::fs::write(
        dir.join("intrinsics.json"),
        format!(
            "{{\"width\":{},\"height\":{},\"focal\":{},\"rect_rotation\":[{}],\"origin\":[{},{},{}]}}",
            config.width, config.height, matcher.focal,
            r.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","),
            matcher.origin[0], matcher.origin[1], matcher.origin[2]
        ),
    )?;

    let mut device = xvisio::Device::open_first()?;
    let stream = device.start_slam_with_cameras(xvisio::SlamMode::Edge)?;
    let mut camera = StereoCamera::open()?;
    let start = Instant::now();

    let poses = Arc::new(Mutex::new(std::io::BufWriter::new(std::fs::File::create(dir.join("poses.csv"))?)));
    let sink = poses.clone();
    std::thread::spawn(move || {
        while let Ok(s) = stream.recv() {
            let p = s.pose;
            let _ = writeln!(
                sink.lock().unwrap(),
                "{:.3},{},{},{},{},{},{},{},{:.3}",
                start.elapsed().as_secs_f64() * 1e3,
                p.translation[0], p.translation[1], p.translation[2],
                p.quaternion[0], p.quaternion[1], p.quaternion[2], p.quaternion[3],
                p.confidence
            );
        }
    });

    let mut frames = std::io::BufWriter::new(std::fs::File::create(dir.join("frames.csv"))?);
    let mut n = 0u32;
    while start.elapsed() < Duration::from_secs(seconds) {
        let f = camera.frame()?;
        let received_ms = (f.received - start).as_secs_f64() * 1e3;
        let (left, right, depth) = matcher.depth_from(&f.images[0], &f.images[1]);
        std::fs::write(dir.join(format!("images/{n}.bin")), [left, right].concat())?;
        let mm: Vec<u8> = depth.iter().flat_map(|d| (if d.is_finite() { (d * 1000.0).round().clamp(0.0, 65535.0) as u16 } else { 0 }).to_le_bytes()).collect();
        std::fs::write(dir.join(format!("depth/{n}.bin")), mm)?;
        writeln!(frames, "{n},{received_ms:.3}")?;
        n += 1;
    }
    frames.flush()?;
    poses.lock().unwrap().flush()?;
    println!("recorded {n} depth frames in {}", dir.display());
    Ok(())
}
