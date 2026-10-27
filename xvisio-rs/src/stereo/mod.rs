//! Depth from the XR50's stereo fisheye pair, computed on the host.
//!
//! The device has no depth unit (features: SGBM off), so: each fisheye image is rectified
//! into a shared pinhole view with the baseline along x (lookup tables built once from the
//! fitted [`Eucm`] models and the cameras' poses), matched with a census transform and
//! box-aggregated Hamming cost (rows in parallel), filtered with a left-right consistency
//! check and a speckle filter, and refined to
//! sub-pixel, then converted to metres. Pure Rust, no platform code.
//!
//! The calibration comes from the device through the official SDK once
//! (`xvsdk-bridge --dump-calibration > calibration/xr50-fisheye.json`) and is plain data
//! from then on, usable on every platform.
pub mod eucm;

pub use eucm::Eucm;
use rayon::prelude::*;

type V3 = [f64; 3];

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add_scaled(a: V3, b: V3, s: f64) -> V3 {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}
/// Row-major 3x3 times vector, and its transpose.
fn mul(m: &[f64; 9], v: V3) -> V3 {
    [m[0] * v[0] + m[1] * v[1] + m[2] * v[2], m[3] * v[0] + m[4] * v[1] + m[5] * v[2], m[6] * v[0] + m[7] * v[1] + m[8] * v[2]]
}
fn mul_t(m: &[f64; 9], v: V3) -> V3 {
    [m[0] * v[0] + m[3] * v[1] + m[6] * v[2], m[1] * v[0] + m[4] * v[1] + m[7] * v[2], m[2] * v[0] + m[5] * v[1] + m[8] * v[2]]
}

/// One fisheye camera: intrinsics and its pose in the IMU frame (`p_imu = R p_cam + t`).
#[derive(Clone, Debug)]
pub struct Camera {
    pub model: Eucm,
    pub rotation: [f64; 9],
    pub translation: V3,
    /// RMS reprojection error of the fit against the SDK's rays, pixels.
    pub fit_rms: f64,
}

/// Both cameras, as in the calibration file (index 0 and 1).
#[derive(Clone, Debug)]
pub struct Calibration {
    pub cameras: [Camera; 2],
}

impl Calibration {
    /// Reads `xvsdk-bridge --dump-calibration` output and fits an EUCM model per camera.
    pub fn from_json(text: &str) -> Result<Calibration, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let cams = v["cameras"].as_array().ok_or("no cameras")?;
        if cams.len() < 2 {
            return Err("need two cameras".into());
        }
        let floats = |x: &serde_json::Value| -> Vec<f64> { x.as_array().map(|a| a.iter().filter_map(|f| f.as_f64()).collect()).unwrap_or_default() };
        let camera = |c: &serde_json::Value| -> Result<Camera, String> {
            let model = &c["models"][0];
            let (w, h) = (model["w"].as_f64().ok_or("model width")?, model["h"].as_f64().ok_or("model height")?);
            let samples: Vec<([f64; 2], V3)> = model["rays"]
                .as_array()
                .ok_or("no rays")?
                .iter()
                .map(|r| floats(r))
                .filter(|r| r.len() == 5)
                .map(|r| ([r[0], r[1]], norm([r[2], r[3], r[4]])))
                .collect();
            let (model, fit_rms) = Eucm::fit(&samples, w, h);
            let rotation: [f64; 9] = floats(&c["rotation"]).try_into().map_err(|_| "rotation")?;
            let translation: V3 = floats(&c["translation"]).try_into().map_err(|_| "translation")?;
            Ok(Camera { model, rotation, translation, fit_rms })
        };
        Ok(Calibration { cameras: [camera(&cams[0])?, camera(&cams[1])?] })
    }

    /// Distance between the camera centres, metres.
    pub fn baseline(&self) -> f64 {
        let b = sub(self.cameras[1].translation, self.cameras[0].translation);
        dot(b, b).sqrt()
    }
}

/// Settings of the rectified pinhole view and the matcher.
#[derive(Clone, Copy, Debug)]
pub struct StereoConfig {
    pub width: usize,
    pub height: usize,
    /// Horizontal field of view of the rectified view, degrees.
    pub hfov_deg: f64,
    /// Disparity search range, pixels (sets the closest measurable depth).
    pub max_disparity: usize,
    /// SGM penalties for a one-pixel disparity step and for a larger jump.
    pub p1: u16,
    pub p2: u16,
    /// The best match must beat the runner-up (outside +-1 px) by this factor.
    pub uniqueness: f32,
    /// Connected regions of similar disparity smaller than this are dropped (0 = off).
    pub speckle_size: usize,
}

impl Default for StereoConfig {
    fn default() -> Self {
        StereoConfig { width: 320, height: 240, hfov_deg: 100.0, max_disparity: 48, p1: 8, p2: 96, uniqueness: 1.1, speckle_size: 80 }
    }
}

/// Rectification maps and matcher for one calibrated pair.
pub struct StereoMatcher {
    pub config: StereoConfig,
    /// Focal length of the rectified view, pixels.
    pub focal: f64,
    pub baseline: f64,
    /// Rectified-frame axes in IMU coordinates (rows: x along the baseline, y down, z forward).
    pub rect_rotation: [f64; 9],
    /// Origin of the rectified frame (the left camera's centre) in IMU coordinates.
    pub origin: V3,
    /// Which calibration camera is on the left as the cameras look (0 or 1).
    pub left: usize,
    /// Per rectified pixel, the source pixel in each camera (NaN outside its image).
    maps: [Vec<[f32; 2]>; 2],
    source: (usize, usize),
}

/// A disparity map in the rectified left view (NaN where no reliable match was found).
pub struct Disparity {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

impl StereoMatcher {
    /// Rectified frame → IMU frame (the depth camera's pose on the device).
    pub fn rect_to_imu(&self) -> crate::recon::Rigid {
        let r = self.rect_rotation.map(|v| v as f32);
        crate::recon::Rigid { rotation: [[r[0], r[3], r[6]], [r[1], r[4], r[7]], [r[2], r[5], r[8]]], translation: self.origin.map(|v| v as f32) }
    }

    /// Builds the rectification for images of `source` size (640x400 on the XR50).
    pub fn new(calibration: &Calibration, source: (usize, usize), config: StereoConfig) -> StereoMatcher {
        // Left is the camera on the left as the cameras look (their own +x is "right"), so the
        // rectified view is upright and disparities are positive.
        let cams = &calibration.cameras;
        let towards_1 = norm(sub(cams[1].translation, cams[0].translation));
        let left = if dot(towards_1, mul(&cams[0].rotation, [1.0, 0.0, 0.0])) > 0.0 { 0 } else { 1 };
        let (c0, c1) = (&cams[left], &cams[1 - left]);
        let x = norm(sub(c1.translation, c0.translation));
        let forward = norm(add_scaled(mul(&c0.rotation, [0.0, 0.0, 1.0]), mul(&c1.rotation, [0.0, 0.0, 1.0]), 1.0));
        let z = norm(add_scaled(forward, x, -dot(forward, x)));
        let y = cross(z, x);
        let rect_rotation = [x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1], z[2]];
        let focal = config.width as f64 / 2.0 / (config.hfov_deg.to_radians() / 2.0).tan();
        let (cx, cy) = (config.width as f64 / 2.0, config.height as f64 / 2.0);

        let map = |cam: &Camera| -> Vec<[f32; 2]> {
            let mut out = Vec::with_capacity(config.width * config.height);
            for v in 0..config.height {
                for u in 0..config.width {
                    let d_rect = [(u as f64 - cx) / focal, (v as f64 - cy) / focal, 1.0];
                    let d_imu = mul_t(&rect_rotation, d_rect);
                    let d_cam = mul_t(&cam.rotation, d_imu);
                    let px = cam.model.project(d_cam).filter(|p| p[0] >= 0.0 && p[1] >= 0.0 && p[0] < (source.0 - 1) as f64 && p[1] < (source.1 - 1) as f64);
                    out.push(px.map_or([f32::NAN; 2], |p| [p[0] as f32, p[1] as f32]));
                }
            }
            out
        };
        StereoMatcher {
            config,
            focal,
            baseline: calibration.baseline(),
            rect_rotation,
            origin: c0.translation,
            left,
            maps: [map(c0), map(c1)], // c0 is already the left camera
            source,
        }
    }

    /// Bilinear resampling of one 8-bit source image (calibration camera `camera`) into the
    /// rectified view.
    pub fn rectify(&self, camera: usize, image: &[u8]) -> Vec<u8> {
        let w = self.source.0;
        self.maps[(camera != self.left) as usize]
            .iter()
            .map(|&[x, y]| {
                if x.is_nan() {
                    return 0;
                }
                let (x0, y0) = (x as usize, y as usize);
                let (fx, fy) = (x - x0 as f32, y - y0 as f32);
                let p = |xx: usize, yy: usize| image[yy * w + xx] as f32;
                let top = p(x0, y0) * (1.0 - fx) + p(x0 + 1, y0) * fx;
                let bottom = p(x0, y0 + 1) * (1.0 - fx) + p(x0 + 1, y0 + 1) * fx;
                (top * (1.0 - fy) + bottom * fy).round() as u8
            })
            .collect()
    }

    /// Rectifies both calibration cameras' images and matches them.
    pub fn depth_from(&self, camera0: &[u8], camera1: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<f32>) {
        let (l, r) = if self.left == 0 { (camera0, camera1) } else { (camera1, camera0) };
        let (left, right) = (self.rectify(self.left, l), self.rectify(1 - self.left, r));
        let depth = self.depth(&self.disparity(&left, &right));
        (left, right, depth)
    }

    /// Disparity of a rectified pair (as produced by [`Self::rectify`] for the left and
    /// right camera).
    pub fn disparity(&self, left: &[u8], right: &[u8]) -> Disparity {
        let StereoConfig { width: w, height: h, max_disparity: dmax, p1, p2, uniqueness, speckle_size, .. } = self.config;
        let (cl, cr) = (census(left, w, h), census(right, w, h));

        // Cost volume (row-major, dmax per pixel): Hamming distance of census codes.
        let mut cost = vec![INVALID; w * h * dmax];
        cost.par_chunks_mut(w * dmax).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                for d in 0..dmax.min(x + 1) {
                    row[x * dmax + d] = (cl[y * w + x] ^ cr[y * w + x - d]).count_ones();
                }
            }
        });
        let cost = sgm(&cost, w, h, dmax, p1, p2);

        // Left: winner takes all with a uniqueness test and parabola sub-pixel refinement.
        // Right: winner takes all only, for the left-right consistency check.
        let rows: Vec<(Vec<f32>, Vec<usize>)> = (0..h)
            .into_par_iter()
            .map(|y| {
                let at = |x: usize| &cost[(y * w + x) * dmax..(y * w + x + 1) * dmax];
                let left_d = (0..w)
                    .map(|x| {
                        let c = at(x);
                        let (best, &cb) = c.iter().enumerate().min_by_key(|(_, v)| **v).unwrap();
                        if cb == INVALID || best == 0 || best + 1 >= dmax {
                            return f32::NAN;
                        }
                        let second = c.iter().enumerate().filter(|(d, _)| d.abs_diff(best) > 1).map(|(_, v)| *v).min().unwrap_or(INVALID);
                        if (second as f32) < cb as f32 * uniqueness {
                            return f32::NAN; // ambiguous: textureless or repetitive
                        }
                        let (a, b, cc) = (c[best - 1] as f32, cb as f32, c[best + 1] as f32);
                        let denom = a - 2.0 * b + cc;
                        best as f32 + if denom > 0.0 { 0.5 * (a - cc) / denom } else { 0.0 }
                    })
                    .collect();
                let right_d = (0..w)
                    .map(|xr| (0..dmax.min(w - xr)).min_by_key(|&d| at(xr + d)[d]).unwrap_or(usize::MAX))
                    .collect();
                (left_d, right_d)
            })
            .collect();

        let mut values: Vec<f32> = rows
            .par_iter()
            .flat_map_iter(|(left_d, right_d)| {
                left_d.iter().enumerate().map(move |(x, &d)| {
                    let xr = x as isize - d.round() as isize;
                    let consistent = d.is_finite() && xr >= 0 && right_d[xr as usize].abs_diff(d.round() as usize) <= 1;
                    if consistent { d } else { f32::NAN }
                })
            })
            .collect();
        if speckle_size > 0 {
            remove_speckles(&mut values, w, h, speckle_size, 1.0);
        }
        Disparity { width: w, height: h, values }
    }

    /// Depth along the rectified z axis, metres (NaN where unknown).
    pub fn depth(&self, disparity: &Disparity) -> Vec<f32> {
        let fb = (self.focal * self.baseline) as f32;
        disparity.values.iter().map(|&d| if d > 0.5 { fb / d } else { f32::NAN }).collect()
    }

    /// 3D points in the IMU frame (metres) for every pixel with a depth, every `step` pixels.
    pub fn points(&self, depth: &[f32], step: usize) -> Vec<V3> {
        let StereoConfig { width: w, height: h, .. } = self.config;
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        (0..h)
            .step_by(step)
            .flat_map(|v| (0..w).step_by(step).map(move |u| (u, v)))
            .filter_map(|(u, v)| {
                let z = depth[v * w + u] as f64;
                if !z.is_finite() {
                    return None;
                }
                let p_rect = [(u as f64 - cx) / self.focal * z, (v as f64 - cy) / self.focal * z, z];
                Some(add_scaled(self.origin, mul_t(&self.rect_rotation, p_rect), 1.0))
            })
            .collect()
    }
}

const INVALID: u32 = u32::MAX;

/// 7x7 census transform (48 bits) of the centre pixel against its neighbours.
fn census(img: &[u8], w: usize, h: usize) -> Vec<u64> {
    let mut out = vec![0u64; w * h];
    out.par_chunks_mut(w).enumerate().skip(3).take(h.saturating_sub(6)).for_each(|(y, row)| {
        for x in 3..w.saturating_sub(3) {
            let c = img[y * w + x];
            let mut bits = 0u64;
            for dy in 0..7 {
                for dx in 0..7 {
                    if dy != 3 || dx != 3 {
                        bits = (bits << 1) | (img[(y + dy - 3) * w + x + dx - 3] < c) as u64;
                    }
                }
            }
            row[x] = bits;
        }
    });
    out
}

/// Semi-global matching (Hirschmüller 2008) along four paths (left, right, down, up): each
/// path cost adds the pixel's matching cost to the cheapest previous-pixel state, paying `p1`
/// for a one-pixel disparity step and `p2` for a larger jump, so smooth surfaces win on weak
/// texture while depth edges stay sharp. Returns the summed path costs; disparities past the
/// image edge (d > x) are INVALID.
fn sgm(cost: &[u32], w: usize, h: usize, dmax: usize, p1: u16, p2: u16) -> Vec<u32> {
    let clamped: Vec<u16> = cost.par_iter().map(|&c| if c == INVALID { MAX_COST } else { c as u16 }).collect();
    let rows = scan_rows(&clamped, w, h, dmax, p1, p2);
    let cols = transpose(&scan_rows(&transpose(&clamped, w, h, dmax), h, w, dmax, p1, p2), h, w, dmax);
    let mut out: Vec<u32> = rows.par_iter().zip(&cols).map(|(&a, &b)| a as u32 + b as u32).collect();
    out.par_chunks_mut(w * dmax).for_each(|row| {
        for x in 0..w.min(dmax) {
            row[x * dmax + x + 1..(x + 1) * dmax].fill(INVALID);
        }
    });
    out
}

/// Census costs are at most 48 bits; invalid ones cost a bit more than any real match.
const MAX_COST: u16 = 64;

/// Both horizontal SGM paths of every row, summed.
fn scan_rows(cost: &[u16], w: usize, h: usize, dmax: usize, p1: u16, p2: u16) -> Vec<u16> {
    let mut out = vec![0u16; w * h * dmax];
    out.par_chunks_mut(w * dmax).zip(cost.par_chunks(w * dmax)).for_each(|(acc, row)| {
        let (mut prev, mut next) = (vec![0u16; dmax], vec![0u16; dmax]);
        for forward in [true, false] {
            prev.fill(0);
            let mut prev_min = 0u16;
            for i in 0..w {
                let x = if forward { i } else { w - 1 - i };
                let c = &row[x * dmax..(x + 1) * dmax];
                let jump = prev_min + p2;
                let mut min = u16::MAX;
                for d in 0..dmax {
                    let mut best = prev[d].min(jump);
                    if d > 0 {
                        best = best.min(prev[d - 1] + p1);
                    }
                    if d + 1 < dmax {
                        best = best.min(prev[d + 1] + p1);
                    }
                    let l = c[d] + best - prev_min;
                    next[d] = l;
                    min = min.min(l);
                }
                for (a, l) in acc[x * dmax..(x + 1) * dmax].iter_mut().zip(&next) {
                    *a += l;
                }
                std::mem::swap(&mut prev, &mut next);
                prev_min = min;
            }
        }
    });
    out
}

/// Swaps the two image axes of a `w` x `h` x `dmax` volume.
fn transpose(v: &[u16], w: usize, h: usize, dmax: usize) -> Vec<u16> {
    let mut out = vec![0u16; v.len()];
    out.par_chunks_mut(h * dmax).enumerate().for_each(|(x, col)| {
        for y in 0..h {
            col[y * dmax..(y + 1) * dmax].copy_from_slice(&v[(y * w + x) * dmax..(y * w + x + 1) * dmax]);
        }
    });
    out
}

/// Drops connected regions (4-neighbour, disparity steps <= `max_diff`) smaller than
/// `min_size` pixels, like OpenCV's filterSpeckles.
fn remove_speckles(d: &mut [f32], w: usize, h: usize, min_size: usize, max_diff: f32) {
    let mut label = vec![0u32; w * h];
    let mut next = 0u32;
    let mut stack = Vec::new();
    let mut region = Vec::new();
    for start in 0..w * h {
        if d[start].is_nan() || label[start] != 0 {
            continue;
        }
        next += 1;
        label[start] = next;
        stack.push(start);
        region.clear();
        while let Some(i) = stack.pop() {
            region.push(i);
            let (x, y) = (i % w, i / w);
            let neighbours = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            for j in neighbours.into_iter().flatten() {
                if label[j] == 0 && d[j].is_finite() && (d[j] - d[i]).abs() <= max_diff {
                    label[j] = next;
                    stack.push(j);
                }
            }
        }
        if region.len() < min_size {
            for &i in &region {
                d[i] = f32::NAN;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A textured plane at a known depth, seen by two ideal pinhole cameras on a baseline,
    /// must come back at that depth.
    #[test]
    fn synthetic_plane_depth() {
        let (w, h, f, b, z) = (160usize, 120usize, 80.0f64, 0.05f64, 0.5f64);
        let texture = |x: f64, y: f64| -> u8 { ((((x * 97.0).sin() * (y * 61.0).cos() + (x * 31.0 + y * 17.0).sin()) * 60.0) + 128.0) as u8 };
        let render = |cam_x: f64| -> Vec<u8> {
            (0..h).flat_map(|v| (0..w).map(move |u| (u, v))).map(|(u, v)| {
                let (rx, ry) = ((u as f64 - w as f64 / 2.0) / f, (v as f64 - h as f64 / 2.0) / f);
                texture(cam_x + rx * z, ry * z)
            }).collect()
        };
        let config = StereoConfig { width: w, height: h, hfov_deg: 2.0 * (w as f64 / 2.0 / f).atan().to_degrees(), max_disparity: 24, ..StereoConfig::default() };
        let matcher = StereoMatcher {
            config,
            focal: f,
            baseline: b,
            rect_rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            origin: [0.0; 3],
            left: 0,
            maps: [vec![], vec![]],
            source: (w, h),
        };
        let disparity = matcher.disparity(&render(0.0), &render(b));
        let depths: Vec<f32> = matcher.depth(&disparity).into_iter().filter(|d| d.is_finite()).collect();
        assert!(depths.len() > w * h / 2, "only {} valid", depths.len());
        let mut sorted = depths.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = sorted[sorted.len() / 2] as f64;
        assert!((median - z).abs() < 0.02, "median depth {median}");
    }
}
