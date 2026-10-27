//! Live depth from the XR50's stereo fisheye pair: rectifies, matches and prints the depth
//! rate, coverage and the median depth in the centre of the view; writes the last rectified
//! pair and depth map as PGM images (near = bright).
//!
//!   cargo run --release --features camera --example depth -- [seconds] [calibration.json] [out-dir]
use std::time::{Duration, Instant};
use xvisio::camera::StereoCamera;
use xvisio::stereo::{Calibration, StereoConfig, StereoMatcher};

fn write_pgm(path: &str, w: usize, h: usize, pixels: &[u8]) -> std::io::Result<()> {
    let mut data = format!("P5\n{w} {h}\n255\n").into_bytes();
    data.extend_from_slice(pixels);
    std::fs::write(path, data)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let calibration_path = args.get(2).cloned().unwrap_or_else(|| "../calibration/xr50-fisheye.json".into());
    let out = args.get(3).cloned().unwrap_or_else(|| ".".into());

    let calibration = Calibration::from_json(&std::fs::read_to_string(&calibration_path)?)?;
    for (i, c) in calibration.cameras.iter().enumerate() {
        println!("camera {i}: EUCM fit {:.3} px rms  {:?}", c.fit_rms, c.model);
    }
    println!("baseline {:.1} mm", calibration.baseline() * 1e3);
    // XVISIO_STEREO="p1,p2,uniqueness,speckle" overrides the matcher settings for tuning.
    let mut config = StereoConfig::default();
    if let Ok(v) = std::env::var("XVISIO_STEREO") {
        let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [p1, p2, u, sp] = p[..] {
            (config.p1, config.p2, config.uniqueness, config.speckle_size) = (p1 as u16, p2 as u16, u, sp as usize);
        }
    }
    let matcher = StereoMatcher::new(&calibration, (640, 400), config);
    let (w, h) = (config.width, config.height);

    let mut device = xvisio::Device::open_first()?;
    let _slam = device.start_slam_with_cameras(xvisio::SlamMode::Edge)?;
    let mut camera = StereoCamera::open()?;

    // Which half of the frame is calibration camera 0: the pairing that matches more pixels.
    let first = camera.frame()?;
    let valid = |swap: bool| {
        let (a, b) = if swap { (1, 0) } else { (0, 1) };
        matcher.depth_from(&first.images[a], &first.images[b]).2.iter().filter(|v| v.is_finite()).count()
    };
    let (straight, swapped) = (valid(false), valid(true));
    let swap = swapped > straight;
    println!("pairing: frame {} = camera 0 ({} vs {} matched pixels)", if swap { "bottom" } else { "top" }, straight.max(swapped), straight.min(swapped));

    let (start, mut frames) = (Instant::now(), 0u32);
    let mut compute = Duration::ZERO;
    let mut last = None;
    while start.elapsed() < Duration::from_secs(seconds) {
        let f = camera.frame()?;
        let t = Instant::now();
        let (a, b) = if swap { (1, 0) } else { (0, 1) };
        let (left, right, depth) = matcher.depth_from(&f.images[a], &f.images[b]);
        compute += t.elapsed();
        frames += 1;
        if frames % 10 == 0 {
            let mut centre: Vec<f32> = (h / 2 - 10..h / 2 + 10)
                .flat_map(|y| (w / 2 - 10..w / 2 + 10).map(move |x| (x, y)))
                .map(|(x, y)| depth[y * w + x])
                .filter(|d| d.is_finite())
                .collect();
            centre.sort_by(|a, b| a.total_cmp(b));
            let coverage = depth.iter().filter(|d| d.is_finite()).count() as f64 / (w * h) as f64;
            println!(
                "coverage {:>3.0}%  centre depth {}",
                coverage * 100.0,
                centre.get(centre.len() / 2).map_or("-".into(), |d| format!("{d:.2} m ({} px)", centre.len())),
            );
        }
        last = Some((left, right, depth));
    }
    let s = start.elapsed().as_secs_f64();
    println!("{frames} frames, {:.1} depth maps/s, {:.0} ms compute per frame", frames as f64 / s, compute.as_secs_f64() * 1e3 / frames.max(1) as f64);

    if let Some((left, right, depth)) = last {
        let pair: Vec<u8> = (0..h).flat_map(|y| left[y * w..(y + 1) * w].iter().chain(&right[y * w..(y + 1) * w]).copied().collect::<Vec<_>>()).collect();
        write_pgm(&format!("{out}/rectified.pgm"), 2 * w, h, &pair)?;
        // 0.2 m → 255, 3 m → 0; unknown → 0
        let shade: Vec<u8> = depth.iter().map(|&d| if d.is_finite() { (255.0 * (1.0 - ((d - 0.2) / 2.8).clamp(0.0, 1.0))) as u8 } else { 0 }).collect();
        write_pgm(&format!("{out}/depth.pgm"), w, h, &shade)?;
        println!("wrote {out}/rectified.pgm and {out}/depth.pgm");
    }
    Ok(())
}
