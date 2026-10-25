struct Camera { view_proj: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexIn {
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) m0: vec4<f32>,
  @location(3) m1: vec4<f32>,
  @location(4) m2: vec4<f32>,
  @location(5) m3: vec4<f32>,
  // alpha 0 = unlit (grid, lines), 1 = lit
  @location(6) color: vec4<f32>,
}

struct VertexOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
  let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
  let world = model * vec4<f32>(v.position, 1.0);
  let normal = normalize((model * vec4<f32>(v.normal, 0.0)).xyz);
  let light = normalize(vec3<f32>(0.4, 0.8, 0.4));
  let lit = 0.45 + 0.55 * max(dot(normal, light), 0.0);
  var out: VertexOut;
  out.clip = camera.view_proj * world;
  out.color = v.color.rgb * mix(1.0, lit, v.color.a);
  return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
  return vec4<f32>(in.color, 1.0);
}
