//! Plane detection on the fused mesh: a Hough-style vote over (normal direction, offset)
//! followed by least-squares refinement on the largest connected region of inliers (a 5 cm
//! occupancy grid on the plane, so separate surfaces at the same height stay separate planes).
//! The area is the region's occupied cells; the border is its convex hull. Linear in the number
//! of vertices, so it can run on every mesh update.
use super::{Mesh, V3};
use rustc_hash::FxHashMap;

/// A detected plane: `normal · p = offset` (unit normal), its border polygon (convex hull of
/// the inliers, in world coordinates, counter-clockwise around the normal) and area (occupied,
/// not hull).
#[derive(Clone, Debug)]
pub struct Plane {
    pub normal: V3,
    pub offset: f32,
    pub border: Vec<V3>,
    pub area: f32,
    pub inliers: usize,
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn normalize(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    if n > 0.0 { [a[0] / n, a[1] / n, a[2] / n] } else { a }
}

#[derive(Clone, Copy, Debug)]
pub struct PlaneConfig {
    /// Inlier distance to the plane, metres.
    pub distance: f32,
    /// Inlier normal agreement, degrees.
    pub angle_deg: f32,
    /// Smallest plane reported, m².
    pub min_area: f32,
    /// Keep only floors/tables and walls: normals within this many degrees of the world's
    /// vertical (+-y, gravity in the XR50 SLAM world) or of horizontal, like ARKit's plane
    /// types. None keeps every orientation.
    pub aligned_deg: Option<f32>,
}

impl Default for PlaneConfig {
    fn default() -> Self {
        PlaneConfig { distance: 0.03, angle_deg: 20.0, min_area: 0.1, aligned_deg: Some(10.0) }
    }
}

/// Normal-direction bin on a cube map (6 faces x 8 x 8), offset bin 2 cm.
fn bins(n: V3, p: V3) -> ([i32; 3], i32) {
    let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
    let (face, u, v) = if ax >= ay && ax >= az {
        (if n[0] > 0.0 { 0 } else { 1 }, n[1] / ax, n[2] / ax)
    } else if ay >= az {
        (if n[1] > 0.0 { 2 } else { 3 }, n[0] / ay, n[2] / ay)
    } else {
        (if n[2] > 0.0 { 4 } else { 5 }, n[0] / az, n[1] / az)
    };
    let q = |x: f32| (((x + 1.0) * 4.0) as i32).clamp(0, 7);
    ([face, q(u), q(v)], (dot(n, p) / 0.02).floor() as i32)
}

/// Finds planes in a mesh with normals.
pub fn detect_planes(mesh: &Mesh, config: PlaneConfig) -> Vec<Plane> {
    let cos_tol = config.angle_deg.to_radians().cos();
    // 1. Vote: (normal bin, offset bin) → vertex ids.
    let mut votes: FxHashMap<([i32; 3], i32), Vec<u32>> = FxHashMap::default();
    for (i, (p, n)) in mesh.positions.iter().zip(&mesh.normals).enumerate() {
        if dot(*n, *n) > 0.5 {
            votes.entry(bins(*n, *p)).or_default().push(i as u32);
        }
    }
    let mut candidates: Vec<Vec<u32>> = votes.into_values().filter(|v| v.len() >= 20).collect();
    candidates.sort_by_key(|v| std::cmp::Reverse(v.len()));

    // 2. Refine each peak (largest first) on the vertices not yet taken.
    let mut taken = vec![false; mesh.positions.len()];
    let mut planes = Vec::new();
    for seed in candidates {
        let seed: Vec<u32> = seed.into_iter().filter(|&i| !taken[i as usize]).collect();
        if seed.len() < 20 {
            continue;
        }
        let mut normal = normalize(seed.iter().fold([0.0; 3], |s, &i| {
            let n = mesh.normals[i as usize];
            [s[0] + n[0], s[1] + n[1], s[2] + n[2]]
        }));
        let mut offset = seed.iter().map(|&i| dot(normal, mesh.positions[i as usize])).sum::<f32>() / seed.len() as f32;
        let mut inliers = Vec::new();
        for _ in 0..3 {
            inliers = (0..mesh.positions.len() as u32)
                .filter(|&i| {
                    let i = i as usize;
                    !taken[i] && (dot(normal, mesh.positions[i]) - offset).abs() < config.distance && dot(normal, mesh.normals[i]) > cos_tol
                })
                .collect();
            inliers = largest_region(mesh, &inliers, normal);
            if inliers.len() < 3 {
                break;
            }
            (normal, offset) = fit_plane(mesh, &inliers);
        }
        if inliers.len() < 20 {
            continue;
        }
        let area = occupied_area(mesh, &inliers, normal);
        let (border, _) = hull(mesh, &inliers, normal, offset);
        let aligned = config.aligned_deg.is_none_or(|t| {
            let tilt = normal[1].abs().clamp(0.0, 1.0).acos().to_degrees();
            tilt < t || tilt > 90.0 - t
        });
        if area < config.min_area || !aligned {
            continue;
        }
        for &i in &inliers {
            taken[i as usize] = true;
        }
        planes.push(Plane { normal, offset, border, area, inliers: inliers.len() });
    }
    planes
}

/// Grid cell of the region search, metres.
const CELL: f32 = 0.05;

/// Orthonormal axes spanning the plane.
fn plane_axes(normal: V3) -> (V3, V3) {
    let seed = if normal[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let e1 = normalize(cross(normal, seed));
    (e1, cross(normal, e1))
}

/// Inliers bucketed into CELL-sized squares on the plane.
fn cells(mesh: &Mesh, ids: &[u32], normal: V3) -> FxHashMap<(i32, i32), Vec<u32>> {
    let (e1, e2) = plane_axes(normal);
    let mut grid: FxHashMap<(i32, i32), Vec<u32>> = FxHashMap::default();
    for &i in ids {
        let p = mesh.positions[i as usize];
        grid.entry(((dot(p, e1) / CELL).floor() as i32, (dot(p, e2) / CELL).floor() as i32)).or_default().push(i);
    }
    grid
}

fn occupied_area(mesh: &Mesh, ids: &[u32], normal: V3) -> f32 {
    cells(mesh, ids, normal).len() as f32 * CELL * CELL
}

/// The inliers of the largest 8-connected group of occupied cells.
fn largest_region(mesh: &Mesh, ids: &[u32], normal: V3) -> Vec<u32> {
    let mut grid = cells(mesh, ids, normal);
    let mut best: Vec<u32> = Vec::new();
    while let Some(&start) = grid.keys().next() {
        let (mut region, mut stack) = (Vec::new(), vec![start]);
        while let Some((x, y)) = stack.pop() {
            let Some(members) = grid.remove(&(x, y)) else { continue };
            region.extend(members);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if grid.contains_key(&(x + dx, y + dy)) {
                        stack.push((x + dx, y + dy));
                    }
                }
            }
        }
        if region.len() > best.len() {
            best = region;
        }
    }
    best
}

/// Least-squares plane through the points: the normal is the covariance's smallest
/// eigenvector, taken as the largest cross product of two covariance rows (Eberly).
fn fit_plane(mesh: &Mesh, ids: &[u32]) -> (V3, f32) {
    let n = ids.len() as f32;
    let c = ids.iter().fold([0.0f32; 3], |s, &i| {
        let p = mesh.positions[i as usize];
        [s[0] + p[0] / n, s[1] + p[1] / n, s[2] + p[2] / n]
    });
    let mut cov = [[0.0f32; 3]; 3];
    for &i in ids {
        let p = mesh.positions[i as usize];
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        for a in 0..3 {
            for b in 0..3 {
                cov[a][b] += d[a] * d[b];
            }
        }
    }
    // The normal is orthogonal to the two dominant directions: take the largest cross
    // product of covariance rows (Eberly's method for the smallest eigenvector).
    let candidates = [cross(cov[0], cov[1]), cross(cov[0], cov[2]), cross(cov[1], cov[2])];
    let best = candidates.into_iter().max_by(|a, b| dot(*a, *a).total_cmp(&dot(*b, *b))).unwrap();
    let mut normal = normalize(best);
    // Keep the orientation of the mesh normals.
    let mean_n = ids.iter().fold([0.0; 3], |s, &i| {
        let m = mesh.normals[i as usize];
        [s[0] + m[0], s[1] + m[1], s[2] + m[2]]
    });
    if dot(normal, mean_n) < 0.0 {
        normal = [-normal[0], -normal[1], -normal[2]];
    }
    (normal, dot(normal, c))
}

/// Convex hull (Andrew's monotone chain) of the inliers projected onto the plane.
fn hull(mesh: &Mesh, ids: &[u32], normal: V3, offset: f32) -> (Vec<V3>, f32) {
    let (e1, e2) = plane_axes(normal);
    let mut pts: Vec<[f32; 2]> = ids.iter().map(|&i| {
        let p = mesh.positions[i as usize];
        [dot(p, e1), dot(p, e2)]
    }).collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return (Vec::new(), 0.0);
    }
    let turn = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && turn(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && turn(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    let area = 0.5 * (0..lower.len()).map(|i| {
        let (a, b) = (lower[i], lower[(i + 1) % lower.len()]);
        a[0] * b[1] - a[1] * b[0]
    }).sum::<f32>().abs();
    let origin = [normal[0] * offset, normal[1] * offset, normal[2] * offset];
    let border = lower.iter().map(|q| [origin[0] + e1[0] * q[0] + e2[0] * q[1], origin[1] + e1[1] * q[0] + e2[1] * q[1], origin[2] + e1[2] * q[0] + e2[2] * q[1]]).collect();
    (border, area)
}

#[cfg(test)]
mod tests {
    use crate::recon::{mesh, DepthImage, Rigid, Tsdf, TsdfConfig};

    /// A flat floor seen from a few poses must fuse into one horizontal plane at the right height.
    #[test]
    fn floor_from_depth() {
        let (w, h, f) = (160usize, 120usize, 100.0f32);
        let floor_y = 1.2f32; // camera frame: x right, y down, z forward; floor 1.2 m below
        let mut tsdf = Tsdf::new(TsdfConfig::default());
        for k in 0..12 {
            // Camera looking 40° down from height 0 (world y down), sliding sideways.
            let pitch = 40f32.to_radians();
            let (c, s) = (pitch.cos(), pitch.sin());
            let cam_to_world = Rigid { rotation: [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]], translation: [k as f32 * 0.03, 0.0, 0.0] };
            let depth: Vec<f32> = (0..h).flat_map(|v| (0..w).map(move |u| (u, v))).map(|(u, v)| {
                let r = [(u as f32 - w as f32 / 2.0) / f, (v as f32 - h as f32 / 2.0) / f, 1.0];
                let dir_w_y = c * r[1] + s * r[2]; // world y of the ray
                if dir_w_y > 0.05 { floor_y / dir_w_y } else { f32::NAN }
            }).collect();
            let image = DepthImage { width: w, height: h, focal: f, cx: w as f32 / 2.0, cy: h as f32 / 2.0, depth: &depth };
            tsdf.integrate(&image, &cam_to_world, 1);
        }
        let m = mesh::mesh_all(&tsdf);
        assert!(m.triangle_count() > 500, "{} triangles", m.triangle_count());
        let planes = super::detect_planes(&m, super::PlaneConfig::default());
        let floor = planes.first().expect("a plane");
        assert!(floor.normal[1].abs() > 0.99, "normal {:?}", floor.normal);
        assert!((floor.offset.abs() - floor_y).abs() < 0.03, "offset {}", floor.offset);
        assert!(floor.area > 1.0, "area {}", floor.area);
    }
}
