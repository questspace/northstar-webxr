//! Diagnostic: does edge SLAM track motion while our camera reader streams? Move the headset
//! during the run. Camera enable comes from the environment (XVISIO_UVC_MODE,
//! XVISIO_ENABLE_STEREO_INIT); XVISIO_NO_CAMERA=1 skips reading frames.
use std::time::{Duration, Instant};

const SECONDS: u64 = 10;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut device = xvisio::Device::open_first()?;
    let stream = device.start_slam(xvisio::SlamMode::Edge)?;
    let frames = std::thread::spawn(|| -> usize {
        if std::env::var_os("XVISIO_NO_CAMERA").is_some() {
            return 0;
        }
        let Ok(mut camera) = xvisio::camera::StereoCamera::open() else { return 0 };
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < Duration::from_secs(SECONDS) && camera.frame().is_ok() {
            n += 1;
        }
        n
    });
    let start = Instant::now();
    let (mut poses, mut first, mut max_move, mut max_angle, mut conf) = (0usize, None, 0.0f64, 0.0f64, Vec::new());
    while start.elapsed() < Duration::from_secs(SECONDS) {
        let Ok(s) = stream.recv_timeout(Duration::from_millis(200)) else { continue };
        poses += 1;
        conf.push(s.pose.confidence);
        let (t, q) = (s.pose.translation, s.pose.quaternion);
        let (t0, q0) = *first.get_or_insert((t, q));
        let d = ((t[0] - t0[0]).powi(2) + (t[1] - t0[1]).powi(2) + (t[2] - t0[2]).powi(2)).sqrt();
        let dot = (q[0] * q0[0] + q[1] * q0[1] + q[2] * q0[2] + q[3] * q0[3]).abs().min(1.0);
        max_move = max_move.max(d);
        max_angle = max_angle.max(2.0 * dot.acos().to_degrees());
    }
    println!(
        "uvcMode={} stereoInit={}: camera {} frames, SLAM {} poses, moved up to {:.0} mm and {:.0} deg, median confidence {:.2}",
        std::env::var("XVISIO_UVC_MODE").unwrap_or_else(|_| "0".into()),
        std::env::var("XVISIO_ENABLE_STEREO_INIT").unwrap_or_else(|_| "0".into()),
        frames.join().unwrap_or(0),
        poses,
        max_move * 1000.0,
        max_angle,
        {
            conf.sort_by(f64::total_cmp);
            conf.get(conf.len() / 2).copied().unwrap_or(0.0)
        }
    );
    Ok(())
}
