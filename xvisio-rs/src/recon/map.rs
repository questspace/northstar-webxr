//! A live room map: the TSDF plus one mesh per block, kept current incrementally, and the
//! binary format the bridge streams changed blocks in.
//!
//! Wire format (little-endian): `u32` block count, then per block `i32 x 3` block key,
//! `u32` vertex count `n`, `u32` index count `m`, `f32 x 3n` positions (SLAM world, metres),
//! `u32 x m` triangle indices. A block with no vertices was removed. Every field is 4 bytes,
//! so the arrays can be viewed in place (e.g. `Float32Array` in a browser).
use super::{camera_to_world, mesh, BlockKey, DepthImage, Mesh, Rigid, Tsdf, TsdfConfig};
use crate::stereo::StereoMatcher;
use crate::types::Pose;
use rustc_hash::{FxHashMap, FxHashSet};

pub struct RoomMap {
    pub tsdf: Tsdf,
    meshes: FxHashMap<BlockKey, Mesh>,
}

/// A changed block's quads can sit in the block itself or in its lower neighbours (their
/// cells reach one voxel into it).
const AFFECTED: [[i32; 3]; 8] = [[0, 0, 0], [-1, 0, 0], [0, -1, 0], [0, 0, -1], [-1, -1, 0], [-1, 0, -1], [0, -1, -1], [-1, -1, -1]];

impl RoomMap {
    pub fn new(config: TsdfConfig) -> RoomMap {
        RoomMap { tsdf: Tsdf::new(config), meshes: FxHashMap::default() }
    }

    pub fn integrate(&mut self, depth: &DepthImage, camera_to_world: &Rigid) {
        self.tsdf.integrate(depth, camera_to_world, 2);
    }

    /// Depth from a raw stereo pair (camera 0, camera 1), fused at the edge-SLAM pose it was
    /// captured at.
    pub fn integrate_stereo(&mut self, matcher: &StereoMatcher, images: [&[u8]; 2], pose: &Pose) {
        let (_, _, depth) = matcher.depth_from(images[0], images[1]);
        let (w, h) = (matcher.config.width, matcher.config.height);
        let image = DepthImage { width: w, height: h, focal: matcher.focal as f32, cx: w as f32 / 2.0, cy: h as f32 / 2.0, depth: &depth };
        self.integrate(&image, &camera_to_world(pose, &matcher.rect_to_imu()));
    }

    /// Remeshes what changed since the last call; returns the blocks whose mesh was replaced
    /// or removed.
    pub fn remesh(&mut self) -> Vec<BlockKey> {
        let keys: FxHashSet<BlockKey> = self
            .tsdf
            .take_dirty()
            .into_iter()
            .flat_map(|k| AFFECTED.map(|d| [k[0] + d[0], k[1] + d[1], k[2] + d[2]]))
            .collect();
        let keys: Vec<BlockKey> = keys.into_iter().collect();
        mesh::mesh_blocks(&self.tsdf, &keys)
            .into_iter()
            .filter_map(|(k, m)| {
                if m.indices.is_empty() {
                    self.meshes.remove(&k).map(|_| k)
                } else {
                    self.meshes.insert(k, m);
                    Some(k)
                }
            })
            .collect()
    }

    /// Every meshed block.
    pub fn keys(&self) -> Vec<BlockKey> {
        self.meshes.keys().copied().collect()
    }

    /// All blocks as one mesh (for plane detection or export).
    pub fn mesh(&self) -> Mesh {
        let mut out = Mesh::default();
        for m in self.meshes.values() {
            out.append(m.clone());
        }
        out
    }

    /// The given blocks in the wire format (missing blocks are sent empty: removed).
    pub fn encode(&self, keys: &[BlockKey]) -> Vec<u8> {
        let empty = Mesh::default();
        let mut out = Vec::new();
        out.extend((keys.len() as u32).to_le_bytes());
        for k in keys {
            let m = self.meshes.get(k).unwrap_or(&empty);
            k.iter().for_each(|v| out.extend(v.to_le_bytes()));
            out.extend((m.positions.len() as u32).to_le_bytes());
            out.extend((m.indices.len() as u32).to_le_bytes());
            m.positions.iter().flatten().for_each(|v| out.extend(v.to_le_bytes()));
            m.indices.iter().for_each(|v| out.extend(v.to_le_bytes()));
        }
        out
    }
}
