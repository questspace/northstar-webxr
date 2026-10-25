// NorthStar V2 optics warp — the same mapping as visual-test/src/xr/optics.ts:
// panel pixel (u, v from the top row) → polynomial → ray (rect.y horizontal,
// rect.x vertical) → 70° eye camera → eye texel. Side-by-side eyes in `eyes`.
// The polynomial is static per calibration, so it is evaluated once on the CPU
// into `rays` (one rect per panel pixel); per frame only the re-projection runs.
struct Warp {
  reproject: mat3x3<f32>,
  rect_sign: vec2<f32>,
  tan_half: vec2<f32>,
  offset_left: vec2<f32>,
  offset_right: vec2<f32>,
  strength: f32,
}
@group(0) @binding(0) var<uniform> warp: Warp;
@group(0) @binding(1) var eyes: texture_2d<f32>;
@group(0) @binding(2) var eyes_sampler: sampler;
@group(0) @binding(3) var rays: texture_2d<f32>;

struct VertexOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) uv: vec2<f32>, // GL convention: v = 0 at the bottom row
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
  let pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
  var out: VertexOut;
  out.clip = vec4<f32>(pos[index], 0.0, 1.0);
  out.uv = pos[index] * 0.5 + 0.5;
  return out;
}

fn eye_uv(rect: vec2<f32>, local: vec2<f32>, offset: vec2<f32>) -> vec2<f32> {
  let ray = warp.reproject * vec3<f32>(warp.rect_sign.x * rect.y, warp.rect_sign.y * rect.x, -1.0);
  let uv = 0.5 + 0.5 * ray.xy / (-ray.z * warp.tan_half);
  return mix(local, uv, warp.strength) + offset;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
  let left = in.uv.x < 0.5;
  let local = vec2<f32>(select(in.uv.x * 2.0 - 1.0, in.uv.x * 2.0, left), in.uv.y);
  let dims = vec2<f32>(textureDimensions(rays));
  let rect = textureLoad(rays, vec2<i32>(vec2<f32>(in.uv.x, 1.0 - in.uv.y) * dims), 0).xy;
  let uv = eye_uv(rect, local, select(warp.offset_right, warp.offset_left, left));
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
  }
  // texture space has v = 0 at the top
  let src = vec2<f32>(select(0.5 + uv.x * 0.5, uv.x * 0.5, left), 1.0 - uv.y);
  return textureSample(eyes, eyes_sampler, src);
}
