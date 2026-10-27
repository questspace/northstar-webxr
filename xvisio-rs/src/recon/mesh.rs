//! Surface Nets (Gibson 1998) over the TSDF: one vertex per cell whose corners change sign,
//! placed at the mean of the interpolated edge crossings; one quad per voxel edge that
//! crosses the surface. Smoother than Marching Cubes on noisy fields and table-free.
//! Meshing is per block (in parallel), so a changed block is remeshed on its own.
use super::{BlockKey, Tsdf, V3, BLOCK};
use rayon::prelude::*;
use rustc_hash::FxHashMap;

/// A triangle mesh in world coordinates; normals are the TSDF gradient (outward).
#[derive(Default, Clone)]
pub struct Mesh {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn append(&mut self, other: Mesh) {
        let base = self.positions.len() as u32;
        self.positions.extend(other.positions);
        self.normals.extend(other.normals);
        self.indices.extend(other.indices.into_iter().map(|i| i + base));
    }

    /// ASCII PLY with normals, viewable in MeshLab / Blender.
    pub fn to_ply(&self) -> String {
        let mut s = format!(
            "ply\nformat ascii 1.0\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nproperty float nx\nproperty float ny\nproperty float nz\nelement face {}\nproperty list uchar int vertex_indices\nend_header\n",
            self.positions.len(),
            self.triangle_count()
        );
        for (p, n) in self.positions.iter().zip(&self.normals) {
            s += &format!("{:.4} {:.4} {:.4} {:.3} {:.3} {:.3}\n", p[0], p[1], p[2], n[0], n[1], n[2]);
        }
        for t in self.indices.chunks(3) {
            s += &format!("3 {} {} {}\n", t[0], t[1], t[2]);
        }
        s
    }
}

/// Minimum observation weight for a voxel to take part in the surface (filters one-off noise).
const MIN_WEIGHT: f32 = 2.0;

/// The 8 corners of a cell, as offsets from its minimum voxel.
const CORNERS: [[i32; 3]; 8] = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
/// The 12 cell edges as corner index pairs.
const EDGES: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

/// Meshes the given blocks (cells whose minimum corner lies in them) as one mesh per block.
pub fn mesh_blocks(tsdf: &Tsdf, keys: &[BlockKey]) -> FxHashMap<BlockKey, Mesh> {
    keys.par_iter().map(|&k| (k, mesh_block(tsdf, k))).collect::<Vec<_>>().into_iter().collect()
}

/// Meshes everything.
pub fn mesh_all(tsdf: &Tsdf) -> Mesh {
    let mut out = Mesh::default();
    for (_, m) in mesh_blocks(tsdf, &tsdf.keys()) {
        out.append(m);
    }
    out
}

fn sdf(tsdf: &Tsdf, g: [i32; 3]) -> Option<f32> {
    tsdf.voxel(g).filter(|(_, w)| *w >= MIN_WEIGHT).map(|(d, _)| d)
}

/// Surface Nets vertex of the cell at `g` (minimum corner), if the surface crosses it.
fn cell_vertex(tsdf: &Tsdf, g: [i32; 3]) -> Option<V3> {
    let mut d = [0.0f32; 8];
    for (i, c) in CORNERS.iter().enumerate() {
        d[i] = sdf(tsdf, [g[0] + c[0], g[1] + c[1], g[2] + c[2]])?;
    }
    let (mut sum, mut n) = ([0.0f32; 3], 0);
    for &(a, b) in &EDGES {
        if (d[a] < 0.0) != (d[b] < 0.0) {
            let t = d[a] / (d[a] - d[b]);
            for k in 0..3 {
                sum[k] += CORNERS[a][k] as f32 + t * (CORNERS[b][k] - CORNERS[a][k]) as f32;
            }
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let s = tsdf.config.voxel_size;
    // Voxel centres sit at (index + 0.5) * voxel_size.
    Some([(g[0] as f32 + 0.5 + sum[0] / n as f32) * s, (g[1] as f32 + 0.5 + sum[1] / n as f32) * s, (g[2] as f32 + 0.5 + sum[2] / n as f32) * s])
}

/// TSDF gradient at a world point (central differences on the nearest voxel), normalised.
fn normal_at(tsdf: &Tsdf, p: V3) -> V3 {
    let s = tsdf.config.voxel_size;
    let g = [(p[0] / s - 0.5).round() as i32, (p[1] / s - 0.5).round() as i32, (p[2] / s - 0.5).round() as i32];
    let diff = |axis: usize| {
        let (mut a, mut b) = (g, g);
        a[axis] += 1;
        b[axis] -= 1;
        match (sdf(tsdf, a), sdf(tsdf, b)) {
            (Some(x), Some(y)) => x - y,
            _ => 0.0,
        }
    };
    let n = [diff(0), diff(1), diff(2)];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 0.0] }
}

fn mesh_block(tsdf: &Tsdf, key: BlockKey) -> Mesh {
    let b = BLOCK as i32;
    let base = [key[0] * b, key[1] * b, key[2] * b];
    let mut mesh = Mesh::default();
    // Vertex index per cell, including the one-cell apron below the block that quads need.
    let mut index: FxHashMap<[i32; 3], u32> = FxHashMap::default();
    let mut vertex = |mesh: &mut Mesh, g: [i32; 3]| -> Option<u32> {
        if let Some(&i) = index.get(&g) {
            return Some(i);
        }
        let p = cell_vertex(tsdf, g)?;
        let i = mesh.positions.len() as u32;
        mesh.positions.push(p);
        mesh.normals.push(normal_at(tsdf, p));
        index.insert(g, i);
        Some(i)
    };
    for z in 0..b {
        for y in 0..b {
            for x in 0..b {
                let g = [base[0] + x, base[1] + y, base[2] + z];
                let Some(d0) = sdf(tsdf, g) else { continue };
                // Each voxel edge in +x/+y/+z that crosses the surface gets a quad joining the
                // four cells around that edge.
                for axis in 0..3 {
                    let mut g1 = g;
                    g1[axis] += 1;
                    let Some(d1) = sdf(tsdf, g1) else { continue };
                    if (d0 < 0.0) == (d1 < 0.0) {
                        continue;
                    }
                    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                    let cell = |du: i32, dv: i32| {
                        let mut c = g;
                        c[u] -= du;
                        c[v] -= dv;
                        c
                    };
                    let quad = [cell(1, 1), cell(0, 1), cell(0, 0), cell(1, 0)];
                    let ids: Option<Vec<u32>> = quad.iter().map(|&c| vertex(&mut mesh, c)).collect();
                    let Some(ids) = ids else { continue };
                    // Orient faces so they look out of the surface (towards positive distance).
                    let (a, bb, c, d) = (ids[0], ids[1], ids[2], ids[3]);
                    if d0 < 0.0 {
                        mesh.indices.extend([a, bb, c, a, c, d]);
                    } else {
                        mesh.indices.extend([a, c, bb, a, d, c]);
                    }
                }
            }
        }
    }
    mesh
}
