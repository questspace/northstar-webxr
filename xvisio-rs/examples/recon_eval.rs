//! Offline calibration of a recorded dataset (`recon_record`): which device mounting and frame
//! latency make the SLAM motion agree with the depth? For frame pairs that moved, frame A's
//! depth points are carried by the SLAM motion into frame B's camera and compared with B's
//! measured depth (median |dz|). The right transform minimises it; "static" (no motion) is the
//! baseline any useful pose must beat. Then fuses the best variant into a mesh.
//!
//! When the dataset has the rectified images, depth is recomputed with the current matcher
//! (XVISIO_STEREO="p1,p2,uniqueness,speckle" overrides it) and its coverage reported.
//!
//!   cargo run --release --example recon_eval -- <dataset dir> [out.ply] [calibration.json]
use rayon::prelude::*;
use xvisio::recon::{mesh, DepthImage, Rigid, Tsdf, TsdfConfig};
use xvisio::stereo::{Calibration, StereoConfig, StereoMatcher};

/// Frames between the two views of a pair (~0.2 s at the depth rate).
const GAP: usize = 5;
/// Stereo depth error grows with z²; beyond ~1.2 m it is ±10 cm or worse on this baseline.
const MAX_DEPTH: f32 = 1.2;

struct Pose {
    t_ms: f64,
    pose: Rigid,
    confidence: f64,
}

/// How a SLAM pose and the IMU→device mounting give camera→world.
struct Variant {
    name: String,
    mount: Rigid,
    inverse: bool,
    /// Factor on the SLAM translation (1 = as reported).
    scale: f32,
}

/// The 24 axis-aligned rotations (signed permutation matrices with det +1), named by rows.
fn axis_rotations() -> Vec<(String, [[f32; 3]; 3])> {
    let perms = [[0, 1, 2], [1, 2, 0], [2, 0, 1], [0, 2, 1], [2, 1, 0], [1, 0, 2]];
    let mut out = Vec::new();
    for (pi, p) in perms.iter().enumerate() {
        for signs in 0..8 {
            let s = [0, 1, 2].map(|i| if signs >> i & 1 == 1 { -1.0f32 } else { 1.0 });
            if s[0] * s[1] * s[2] * if pi < 3 { 1.0 } else { -1.0 } < 0.0 {
                continue;
            }
            let mut m = [[0.0f32; 3]; 3];
            let mut name = String::new();
            for i in 0..3 {
                m[i][p[i]] = s[i];
                name += &format!("{}{}", if s[i] < 0.0 { "-" } else { "+" }, ["x", "y", "z"][p[i]]);
            }
            out.push((name, m));
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(args.get(1).ok_or("dataset dir")?);
    let intr: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("intrinsics.json"))?)?;
    let (w, h) = (intr["width"].as_u64().unwrap() as usize, intr["height"].as_u64().unwrap() as usize);
    let focal = intr["focal"].as_f64().unwrap() as f32;
    let r: Vec<f32> = intr["rect_rotation"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    let o: Vec<f32> = intr["origin"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
    let rect_to_imu = Rigid { rotation: [[r[0], r[3], r[6]], [r[1], r[4], r[7]], [r[2], r[5], r[8]]], translation: [o[0], o[1], o[2]] };

    let poses: Vec<Pose> = std::fs::read_to_string(dir.join("poses.csv"))?
        .lines()
        .filter_map(|l| {
            let v: Vec<f64> = l.split(',').filter_map(|x| x.parse().ok()).collect();
            (v.len() == 9).then(|| Pose { t_ms: v[0], pose: Rigid::from_quaternion([v[4], v[5], v[6], v[7]], [v[1], v[2], v[3]]), confidence: v[8] })
        })
        .collect();
    let matcher = dir
        .join("images")
        .is_dir()
        .then(|| -> Result<StereoMatcher, Box<dyn std::error::Error>> {
            let mut config = StereoConfig::default();
            if let Ok(v) = std::env::var("XVISIO_STEREO") {
                if let [p1, p2, u, sp] = v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()[..] {
                    (config.p1, config.p2, config.uniqueness, config.speckle_size) = (p1 as u16, p2 as u16, u, sp as usize);
                }
            }
            let calibration = std::fs::read_to_string(args.get(3).map(String::as_str).unwrap_or("../calibration/xr50-fisheye.json"))?;
            Ok(StereoMatcher::new(&Calibration::from_json(&calibration)?, (640, 400), config))
        })
        .transpose()?;
    let depth_of = |n: &str| -> Option<Vec<f32>> {
        if let Some(m) = &matcher {
            let images = std::fs::read(dir.join(format!("images/{n}.bin"))).ok()?;
            let (left, right) = images.split_at(w * h);
            return Some(m.depth(&m.disparity(left, right)));
        }
        let bytes = std::fs::read(dir.join(format!("depth/{n}.bin"))).ok()?;
        Some(bytes.chunks(2).map(|c| match u16::from_le_bytes([c[0], c[1]]) { 0 => f32::NAN, mm => mm as f32 / 1000.0 }).collect())
    };
    let lines: Vec<String> = std::fs::read_to_string(dir.join("frames.csv"))?.lines().map(String::from).collect();
    let frames: Vec<(f64, Vec<f32>)> = lines
        .iter()
        .filter_map(|l| {
            let (n, t) = l.split_once(',')?;
            Some((t.parse().ok()?, depth_of(n)?))
        })
        .collect();
    let valid: usize = frames.iter().map(|(_, d)| d.iter().filter(|z| z.is_finite()).count()).sum();
    println!(
        "{} poses, {} frames, depth {} with {:.0}% coverage",
        poses.len(),
        frames.len(),
        if matcher.is_some() { "recomputed" } else { "as recorded" },
        100.0 * valid as f64 / (frames.len() * w * h).max(1) as f64
    );

    let pose_at = |t: f64| -> &Pose {
        let i = poses.partition_point(|p| p.t_ms < t).min(poses.len() - 1);
        if i > 0 && (poses[i - 1].t_ms - t).abs() < (poses[i].t_ms - t).abs() { &poses[i - 1] } else { &poses[i] }
    };
    let camera_to_world = |v: &Variant, t: f64| -> Option<Rigid> {
        let p = pose_at(t);
        let pose = Rigid { translation: p.pose.translation.map(|x| x * v.scale), ..p.pose };
        let pose = if v.inverse { pose.inverse() } else { pose };
        (p.confidence >= 0.5).then(|| pose.then(&v.mount).then(&rect_to_imu))
    };
    let unproject = |u: usize, v: usize, z: f32| [(u as f32 - w as f32 / 2.0) / focal * z, (v as f32 - h as f32 / 2.0) / focal * z, z];
    let reference = Variant { name: String::new(), mount: Rigid::IDENTITY, inverse: false, scale: 1.0 };

    // Median |dz| of A's points in B over the moved pairs; `variant: None` ignores the motion.
    let pair_residual = |variant: Option<&Variant>, latency_ms: f64| -> (f64, usize) {
        let cams: Vec<Option<Rigid>> = frames.iter().map(|(t, _)| camera_to_world(variant.unwrap_or(&reference), t - latency_ms)).collect();
        let pairs: Vec<(usize, Rigid)> = (0..frames.len().saturating_sub(GAP))
            .step_by(3)
            .filter_map(|i| {
                let a_to_b = cams[i + GAP].as_ref()?.inverse().then(cams[i].as_ref()?);
                let trace = a_to_b.rotation[0][0] + a_to_b.rotation[1][1] + a_to_b.rotation[2][2];
                let moved = a_to_b.translation.iter().map(|v| v * v).sum::<f32>().sqrt() > 0.03 || trace < 1.0 + 2.0 * 3f32.to_radians().cos();
                (moved != std::env::var_os("XVISIO_STILL").is_some()).then_some((i, if variant.is_some() { a_to_b } else { Rigid::IDENTITY }))
            })
            .collect();
        let mut errors: Vec<f32> = pairs
            .par_iter()
            .flat_map_iter(|(i, a_to_b)| {
                let (da, db) = (&frames[*i].1, &frames[i + GAP].1);
                (0..h).step_by(4).flat_map(move |v| (0..w).step_by(4).map(move |u| (u, v))).filter_map(move |(u, v)| {
                    let z = da[v * w + u];
                    if !(z > 0.2 && z <= MAX_DEPTH) {
                        return None;
                    }
                    let c = a_to_b.apply(unproject(u, v, z));
                    let (ub, vb) = ((c[0] / c[2] * focal + w as f32 / 2.0).round(), (c[1] / c[2] * focal + h as f32 / 2.0).round());
                    if c[2] < 0.2 || ub < 0.0 || vb < 0.0 || ub >= w as f32 || vb >= h as f32 {
                        return None;
                    }
                    let zb = db[vb as usize * w + ub as usize];
                    zb.is_finite().then(|| (zb - c[2]).abs().min(0.5))
                })
            })
            .collect();
        if errors.is_empty() {
            return (f64::NAN, 0);
        }
        let mid = errors.len() / 2;
        errors.select_nth_unstable_by(mid, f32::total_cmp);
        (errors[mid] as f64 * 1000.0, pairs.len())
    };

    let (base, pairs) = pair_residual(None, 0.0);
    println!("static baseline: {base:.1} mm over {pairs} moved pairs\n");
    let variants: Vec<Variant> = axis_rotations()
        .into_iter()
        .flat_map(|(name, m)| [false, true].map(|inverse| Variant { name: format!("{name}{}", if inverse { " inv" } else { "" }), mount: Rigid { rotation: m, translation: [0.0; 3] }, inverse, scale: 1.0 }))
        .collect();
    let mut scored: Vec<(f64, &Variant)> = variants.iter().map(|v| (pair_residual(Some(v), 0.0).0, v)).collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("best mountings (latency 0):");
    for (res, v) in scored.iter().take(6) {
        println!("  {:12} {res:6.1} mm", v.name);
    }
    let best_variant = scored[0].1;
    println!("\nlatency sweep for {}:", best_variant.name);
    let mut best = (f64::MAX, 0.0);
    for latency in (-40..=200).step_by(20).map(f64::from) {
        let (res, _) = pair_residual(Some(best_variant), latency);
        println!("  {latency:>6} ms  {res:6.1} mm");
        if res < best.0 {
            best = (res, latency);
        }
    }
    println!("best: {}, latency {} ms ({:.1} mm, static {base:.1} mm)", best_variant.name, best.1, best.0);

    // Is the SLAM translation metric? Scale it and see which factor the depth agrees with best.
    println!("
translation scale sweep:");
    for scale in [0.5f32, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.5, 2.0] {
        let v = Variant { name: String::new(), mount: best_variant.mount, inverse: best_variant.inverse, scale };
        println!("  x{scale:<4} {:6.1} mm", pair_residual(Some(&v), best.1).0);
    }

    if let Some(out) = args.get(2) {
        let mut tsdf = Tsdf::new(TsdfConfig::default());
        for (t, depth) in &frames {
            if let Some(pose) = camera_to_world(best_variant, t - best.1) {
                tsdf.integrate(&DepthImage { width: w, height: h, focal, cx: w as f32 / 2.0, cy: h as f32 / 2.0, depth }, &pose, 2);
            }
        }
        let m = mesh::mesh_all(&tsdf);
        std::fs::write(out, m.to_ply())?;
        println!("wrote {out} ({} triangles)", m.triangle_count());
        for p in xvisio::recon::detect_planes(&m, Default::default()) {
            println!("  plane {:5.2} m²  normal [{:+.2}, {:+.2}, {:+.2}]  offset {:+.2} m", p.area, p.normal[0], p.normal[1], p.normal[2], p.offset);
        }
    }
    Ok(())
}
