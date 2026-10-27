//! Dense reconstruction from the XR50's depth: a sparse, voxel-hashed TSDF (truncated signed
//! distance field, Curless & Levoy 1996; KinectFusion's projective integration; voxel hashing
//! after Nießner et al. 2013 and VoxBlox), meshed with Surface Nets and searched for planes.
//!
//! Only 8³-voxel blocks near observed surfaces exist, in a hash map keyed by block index, so
//! memory follows the surface area, not the room's volume. Integration and meshing run over
//! blocks in parallel (rayon) and remeshing touches only blocks changed since the last mesh.
//! Everything is in one world frame (the SLAM world); poses come in as camera→world rigid
//! transforms.
pub mod map;
pub mod mesh;
pub mod planes;
pub mod poses;

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

pub use map::RoomMap;
pub use mesh::Mesh;
pub use planes::{detect_planes, Plane};
pub use poses::{PoseHistory, FRAME_DELAY};

pub type V3 = [f32; 3];

/// Voxels per block edge.
pub const BLOCK: usize = 8;
const BLOCK_VOXELS: usize = BLOCK * BLOCK * BLOCK;

/// A rigid transform `p_out = rotation * p_in + translation` (row-major rotation).
#[derive(Clone, Copy, Debug)]
pub struct Rigid {
    pub rotation: [[f32; 3]; 3],
    pub translation: V3,
}

impl Rigid {
    pub const IDENTITY: Rigid = Rigid { rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], translation: [0.0; 3] };

    pub fn apply(&self, p: V3) -> V3 {
        let r = &self.rotation;
        [
            r[0][0] * p[0] + r[0][1] * p[1] + r[0][2] * p[2] + self.translation[0],
            r[1][0] * p[0] + r[1][1] * p[1] + r[1][2] * p[2] + self.translation[1],
            r[2][0] * p[0] + r[2][1] * p[1] + r[2][2] * p[2] + self.translation[2],
        ]
    }

    pub fn inverse(&self) -> Rigid {
        let r = &self.rotation;
        let rt = [[r[0][0], r[1][0], r[2][0]], [r[0][1], r[1][1], r[2][1]], [r[0][2], r[1][2], r[2][2]]];
        let t = self.translation;
        let neg = [
            -(rt[0][0] * t[0] + rt[0][1] * t[1] + rt[0][2] * t[2]),
            -(rt[1][0] * t[0] + rt[1][1] * t[1] + rt[1][2] * t[2]),
            -(rt[2][0] * t[0] + rt[2][1] * t[1] + rt[2][2] * t[2]),
        ];
        Rigid { rotation: rt, translation: neg }
    }

    /// `self ∘ other` (apply `other` first).
    pub fn then(&self, other: &Rigid) -> Rigid {
        let (a, b) = (&self.rotation, &other.rotation);
        let mut r = [[0.0f32; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
            }
        }
        Rigid { rotation: r, translation: self.apply(other.translation) }
    }

    /// From a unit quaternion `[x, y, z, w]` and a translation.
    pub fn from_quaternion(q: [f64; 4], t: [f64; 3]) -> Rigid {
        let [x, y, z, w] = q.map(|v| v as f32);
        Rigid {
            rotation: [
                [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
                [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
                [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
            ],
            translation: t.map(|v| v as f32),
        }
    }
}

/// Depth camera → SLAM world for an edge-SLAM pose. The edge pose is the IMU frame itself in
/// the SLAM world: of all axis-aligned mountings the identity fits recorded depth best
/// (14 mm vs 27+ mm for the next, `examples/recon_eval.rs`).
pub fn camera_to_world(pose: &crate::types::Pose, rect_to_imu: &Rigid) -> Rigid {
    Rigid::from_quaternion(pose.quaternion, pose.translation).then(rect_to_imu)
}

/// A pinhole depth image: `depth[v * width + u]` in metres along +z (NaN = unknown).
pub struct DepthImage<'a> {
    pub width: usize,
    pub height: usize,
    pub focal: f32,
    pub cx: f32,
    pub cy: f32,
    pub depth: &'a [f32],
}

#[derive(Clone, Copy, Default)]
struct Voxel {
    /// Signed distance, metres, clamped to ±truncation.
    sdf: f32,
    /// Observations so far (0 = never seen).
    weight: f32,
}

struct Block {
    voxels: [Voxel; BLOCK_VOXELS],
}

/// Integer block coordinate.
pub type BlockKey = [i32; 3];

#[derive(Clone, Copy, Debug)]
pub struct TsdfConfig {
    /// Voxel edge, metres.
    pub voxel_size: f32,
    /// Truncation distance, metres (surfaces thinner than this merge).
    pub truncation: f32,
    /// Cap on the per-voxel weight, so the map can still change.
    pub max_weight: f32,
    /// Depth beyond this is ignored, metres. Stereo error grows with depth² (±1 cm at 0.5 m,
    /// ±10 cm at 1.5 m on the XR50), so far points only blur the map.
    pub max_depth: f32,
}

impl Default for TsdfConfig {
    fn default() -> Self {
        TsdfConfig { voxel_size: 0.02, truncation: 0.08, max_weight: 64.0, max_depth: 1.5 }
    }
}

/// The sparse TSDF volume.
pub struct Tsdf {
    pub config: TsdfConfig,
    blocks: FxHashMap<BlockKey, Box<Block>>,
    /// Blocks changed since the last `take_dirty`.
    dirty: FxHashSet<BlockKey>,
}

impl Tsdf {
    pub fn new(config: TsdfConfig) -> Tsdf {
        Tsdf { config, blocks: FxHashMap::default(), dirty: FxHashSet::default() }
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    fn block_size(&self) -> f32 {
        self.config.voxel_size * BLOCK as f32
    }

    fn key_of(&self, p: V3) -> BlockKey {
        let s = self.block_size();
        [(p[0] / s).floor() as i32, (p[1] / s).floor() as i32, (p[2] / s).floor() as i32]
    }

    /// Fuses one depth image seen from `camera_to_world`. Returns the number of blocks touched.
    pub fn integrate(&mut self, depth: &DepthImage, camera_to_world: &Rigid, pixel_step: usize) -> usize {
        let TsdfConfig { voxel_size, truncation, max_weight, max_depth } = self.config;

        // 1. Allocate the blocks within the truncation band around every measured point.
        let mut touched: FxHashSet<BlockKey> = FxHashSet::default();
        for v in (0..depth.height).step_by(pixel_step) {
            for u in (0..depth.width).step_by(pixel_step) {
                let z = depth.depth[v * depth.width + u];
                if !(z > 0.0 && z <= max_depth) {
                    continue;
                }
                let ray = [(u as f32 - depth.cx) / depth.focal, (v as f32 - depth.cy) / depth.focal, 1.0];
                for s in [-truncation, 0.0, truncation] {
                    let zz = z + s;
                    if zz > 0.0 {
                        touched.insert(self.key_of(camera_to_world.apply([ray[0] * zz, ray[1] * zz, zz])));
                    }
                }
            }
        }
        for key in &touched {
            self.blocks.entry(*key).or_insert_with(|| Box::new(Block { voxels: [Voxel::default(); BLOCK_VOXELS] }));
        }

        // 2. Projective update of every voxel in those blocks (in parallel over blocks).
        let world_to_camera = camera_to_world.inverse();
        let block_size = self.block_size();
        let (w, h) = (depth.width as f32, depth.height as f32);
        let mut work: Vec<(&BlockKey, &mut Box<Block>)> = self.blocks.iter_mut().filter(|(k, _)| touched.contains(*k)).collect();
        work.par_iter_mut().for_each(|(key, block)| {
            let base = [key[0] as f32 * block_size, key[1] as f32 * block_size, key[2] as f32 * block_size];
            for (i, voxel) in block.voxels.iter_mut().enumerate() {
                let (x, y, z) = (i % BLOCK, (i / BLOCK) % BLOCK, i / (BLOCK * BLOCK));
                let p = [base[0] + (x as f32 + 0.5) * voxel_size, base[1] + (y as f32 + 0.5) * voxel_size, base[2] + (z as f32 + 0.5) * voxel_size];
                let c = world_to_camera.apply(p);
                if c[2] <= 0.05 {
                    continue;
                }
                let u = c[0] / c[2] * depth.focal + depth.cx;
                let v = c[1] / c[2] * depth.focal + depth.cy;
                if u < 0.0 || v < 0.0 || u >= w - 1.0 || v >= h - 1.0 {
                    continue;
                }
                let measured = depth.depth[v.round() as usize * depth.width + u.round() as usize];
                if !(measured > 0.0 && measured <= max_depth) {
                    continue;
                }
                let sdf = measured - c[2];
                if sdf < -truncation {
                    continue; // behind the surface: unknown
                }
                let sdf = sdf.min(truncation);
                // Inverse-variance weight: stereo depth variance grows with z⁴, σ with z² (as in
                // VoxBlox); 1 at 1 m.
                let w = 1.0 / (measured * measured);
                let weight = voxel.weight;
                voxel.sdf = (voxel.sdf * weight + sdf * w) / (weight + w);
                voxel.weight = (weight + w).min(max_weight);
            }
        });
        let n = touched.len();
        self.dirty.extend(touched);
        n
    }

    /// Signed distance and weight at a global voxel index (None if unobserved).
    pub fn voxel(&self, g: [i32; 3]) -> Option<(f32, f32)> {
        let b = BLOCK as i32;
        let key = [g[0].div_euclid(b), g[1].div_euclid(b), g[2].div_euclid(b)];
        let block = self.blocks.get(&key)?;
        let (x, y, z) = (g[0].rem_euclid(b) as usize, g[1].rem_euclid(b) as usize, g[2].rem_euclid(b) as usize);
        let v = block.voxels[x + y * BLOCK + z * BLOCK * BLOCK];
        (v.weight > 0.0).then_some((v.sdf, v.weight))
    }

    /// Signed distance at a world point (nearest voxel), if observed at least `min_weight` times.
    pub fn sdf_at(&self, p: V3, min_weight: f32) -> Option<f32> {
        let s = self.config.voxel_size;
        let g = [(p[0] / s).floor() as i32, (p[1] / s).floor() as i32, (p[2] / s).floor() as i32];
        self.voxel(g).filter(|(_, w)| *w >= min_weight).map(|(d, _)| d)
    }

    /// Blocks changed since the last call (for incremental meshing).
    pub fn take_dirty(&mut self) -> Vec<BlockKey> {
        self.dirty.drain().collect()
    }

    /// All allocated block keys.
    pub fn keys(&self) -> Vec<BlockKey> {
        self.blocks.keys().copied().collect()
    }
}
