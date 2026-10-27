//! Live reconstruction: edge SLAM + stereo depth fused into a room map, meshed incrementally,
//! searched for planes. Writes the mesh as PLY and prints the planes and per-stage timings.
//!
//!   cargo run --release --features camera --example reconstruct -- [seconds] [calibration.json] [out.ply]
//!
//! Move the headset slowly around the room while it runs.
use std::time::{Duration, Instant};
use xvisio::camera::StereoCamera;
use xvisio::recon::{self, PoseHistory, RoomMap, TsdfConfig, FRAME_DELAY};
use xvisio::stereo::{Calibration, StereoConfig, StereoMatcher};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let calibration_path = args.get(2).cloned().unwrap_or_else(|| "../calibration/xr50-fisheye.json".into());
    let out = args.get(3).cloned().unwrap_or_else(|| "room.ply".into());

    let calibration = Calibration::from_json(&std::fs::read_to_string(&calibration_path)?)?;
    let matcher = StereoMatcher::new(&calibration, (640, 400), StereoConfig::default());
    let mut map = RoomMap::new(TsdfConfig::default());

    let mut device = xvisio::Device::open_first()?;
    let stream = device.start_slam_with_cameras(xvisio::SlamMode::Edge)?;
    let mut camera = StereoCamera::open()?;
    let poses = PoseHistory::default();
    let sink = poses.clone();
    std::thread::spawn(move || {
        while let Ok(s) = stream.recv() {
            sink.push(s.pose);
        }
    });

    let (start, mut frames, mut fused) = (Instant::now(), 0u32, 0u32);
    let (mut t_fuse, mut t_mesh) = (Duration::ZERO, Duration::ZERO);
    let mut last_mesh = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        let f = camera.frame()?;
        frames += 1;
        let Some(pose) = poses.at(f.received - FRAME_DELAY) else { continue };
        if pose.confidence < 0.5 {
            continue; // SLAM not tracking: fusing would smear the map
        }
        let t = Instant::now();
        map.integrate_stereo(&matcher, [&f.images[0], &f.images[1]], &pose);
        t_fuse += t.elapsed();
        fused += 1;

        if last_mesh.elapsed() >= Duration::from_secs(1) {
            let t = Instant::now();
            let changed = map.remesh();
            t_mesh += t.elapsed();
            println!(
                "{:>4.0} s  fused {fused}/{frames} frames  blocks {}  remeshed {} in {:.0} ms",
                start.elapsed().as_secs_f64(),
                map.tsdf.block_count(),
                changed.len(),
                t.elapsed().as_secs_f64() * 1e3
            );
            last_mesh = Instant::now();
        }
    }
    let per = |d: Duration| d.as_secs_f64() * 1e3 / fused.max(1) as f64;
    println!("per fused frame: depth + fusion {:.1} ms; meshing {:.0} ms total", per(t_fuse), t_mesh.as_secs_f64() * 1e3);

    map.remesh();
    let t = Instant::now();
    let full = map.mesh();
    let planes = recon::detect_planes(&full, Default::default());
    println!("mesh {} triangles, planes in {:.0} ms:", full.triangle_count(), t.elapsed().as_secs_f64() * 1e3);
    for p in &planes {
        // World is the XR50's SLAM world: gravity along +y (y down).
        let kind = if p.normal[1].abs() > 0.9 { "horizontal" } else { "vertical" };
        println!("  {kind:10} {:5.2} m²  normal [{:+.2}, {:+.2}, {:+.2}]  offset {:+.2} m", p.area, p.normal[0], p.normal[1], p.normal[2], p.offset);
    }
    std::fs::write(&out, full.to_ply())?;
    println!("wrote {out}");
    Ok(())
}
