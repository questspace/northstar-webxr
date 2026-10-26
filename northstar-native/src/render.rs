//! wgpu pipeline: both eyes into one side-by-side texture, then the optics warp
//! onto the swapchain with the freshest head pose folded in as a re-projection.
use crate::calibration::Calibration;
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Quat, Vec3, Vec4};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

const NEAR: f32 = 0.01;
const FAR: f32 = 1000.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

impl Instance {
    pub fn new(model: Mat4, color: Vec3, lit: bool) -> Self {
        Self { model: model.to_cols_array_2d(), color: [color.x, color.y, color.z, if lit { 1.0 } else { 0.0 }] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WarpUniform {
    reproject: [[f32; 4]; 3],
    rect_sign: [f32; 2],
    tan_half: [f32; 2],
    offset_left: [f32; 2],
    offset_right: [f32; 2],
    strength: f32,
    /// 1 = rotate the output 180° (for a panel Windows does not rotate)
    rotate: f32,
    _pad: [f32; 2],
}

pub struct WarpSettings {
    pub strength: f32,
    pub rotate: bool,
    pub rect_sign: [f32; 2],
    pub reproject: Mat3,
}

struct Mesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

fn mesh(device: &wgpu::Device, vertices: &[Vertex], indices: &[u32]) -> Mesh {
    Mesh {
        vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        index_count: indices.len() as u32,
    }
}

fn cube_mesh(device: &wgpu::Device) -> Mesh {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for (n, u, v) in faces {
        let (n, u, v) = (Vec3::from(n), Vec3::from(u), Vec3::from(v));
        let base = vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.push(Vertex { position: ((n + u * su + v * sv) * 0.5).to_array(), normal: n.to_array() });
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh(device, &vertices, &indices)
}

fn sphere_mesh(device: &wgpu::Device, rings: u32, segments: u32) -> Mesh {
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for r in 0..=rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            let n = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            vertices.push(Vertex { position: n.to_array(), normal: n.to_array() });
        }
    }
    for r in 0..rings {
        for s in 0..segments {
            let a = r * (segments + 1) + s;
            let b = a + segments + 1;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    mesh(device, &vertices, &indices)
}

/// numpy polyvander2d row-major: c[i*4+j] * X^i * Y^j
fn poly(c: &[f32; 16], x: f32, y: f32) -> f32 {
    let (xs, ys) = ([1.0, x, x * x, x * x * x], [1.0, y, y * y, y * y * y]);
    (0..4).flat_map(|i| (0..4).map(move |j| (i, j))).map(|(i, j)| c[i * 4 + j] * xs[i] * ys[j]).sum()
}

/// The calibration polynomials evaluated once per panel pixel (row 0 = top row):
/// `(rect.x, rect.y)` per pixel, left half from the left eye's tables.
fn ray_table(c: &Calibration) -> (u32, u32, Vec<f32>) {
    let (w, h) = (c.display.width, c.display.height);
    let half = w / 2;
    let mut table = Vec::with_capacity((w * h * 2) as usize);
    for py in 0..h {
        for px in 0..w {
            let (left, lx) = if px < half { (true, px) } else { (false, px - half) };
            // The fit sampled rows from the top: X = u in the half, Y = v from the top.
            let x = (lx as f32 + 0.5) / half as f32;
            let y = (py as f32 + 0.5) / h as f32;
            let [cx, cy] = if left { [&c.polynomials[0], &c.polynomials[1]] } else { [&c.polynomials[2], &c.polynomials[3]] };
            table.push(poly(cx, x, y));
            table.push(poly(cy, x, y));
        }
    }
    (w, h, table)
}

/// Unit line along +X from the origin.
fn line_mesh(device: &wgpu::Device) -> Mesh {
    let v = |x: f32| Vertex { position: [x, 0.0, 0.0], normal: [0.0, 1.0, 0.0] };
    mesh(device, &[v(0.0), v(1.0)], &[0, 1])
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scene_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    occluder_pipeline: wgpu::RenderPipeline,
    warp_pipeline: wgpu::RenderPipeline,
    camera_buffers: [wgpu::Buffer; 2],
    camera_groups: [wgpu::BindGroup; 2],
    warp_buffer: wgpu::Buffer,
    warp_group: wgpu::BindGroup,
    eyes_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    cube: Mesh,
    sphere: Mesh,
    line: Mesh,
    instances: wgpu::Buffer,
    instance_capacity: u64,
    eye_size: (u32, u32),
    projection: Mat4,
    eye_position: [Vec3; 2],
    warp: WarpUniform,
}

const INSTANCE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<Instance>() as u64,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4],
};

const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<Vertex>() as u64,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
};

/// Draw list for one frame: (mesh kind, instances).
pub enum Kind {
    Cube,
    Sphere,
    Line,
    /// Depth only: cubes that cut holes for the real hands
    Occluder,
}

impl Renderer {
    pub fn new(window: Arc<Window>, calibration: &Calibration, low_power: bool) -> Self {
        let size = window.inner_size();
        // DX12 by default: the Vulkan path dies silently on this Intel+NVIDIA laptop.
        // WGPU_BACKEND=vulkan|dx12 overrides.
        let mut descriptor = wgpu::InstanceDescriptor::from_env_or_default();
        if std::env::var_os("WGPU_BACKEND").is_none() {
            descriptor.backends = wgpu::Backends::DX12;
        }
        let instance = wgpu::Instance::new(&descriptor);
        let surface = instance.create_surface(window).expect("surface");
        // Present on the GPU that owns the panel's output: a cross-adapter present halves the rate.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: if low_power { wgpu::PowerPreference::LowPower } else { wgpu::PowerPreference::HighPerformance },
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("adapter");
        log::info!("GPU: {}", adapter.get_info().name);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .expect("device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            // Frames in flight (NS_FRAME_LATENCY, default 2). With 1 the whole frame must
            // fit one refresh interval or the rate halves (44 fps on the 89 Hz panel);
            // 2 pipelines CPU and GPU at the full rate, and because the pose is sampled
            // after the swapchain acquire its age stays ~1.4 ms either way.
            desired_maximum_frame_latency: std::env::var("NS_FRAME_LATENCY").ok().and_then(|v| v.parse().ok()).unwrap_or(2),
        };
        surface.configure(&device, &config);

        let eye_size = (calibration.display.width / 2, calibration.display.height);
        let eyes = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("eyes"),
            size: wgpu::Extent3d { width: eye_size.0 * 2, height: eye_size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: eye_size.0 * 2, height: eye_size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let camera_buffers = [0, 1].map(|_| device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        let camera_groups = [0, 1].map(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffers[i].as_entire_binding() }],
        }));

        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene.wgsl").into()),
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&uniform_layout],
            push_constant_ranges: &[],
        });
        let scene_pipeline_for = |topology: wgpu::PrimitiveTopology, write_mask: wgpu::ColorWrites| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&scene_layout),
                vertex: wgpu::VertexState {
                    module: &scene_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[VERTEX_LAYOUT, INSTANCE_LAYOUT],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &scene_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask })],
                }),
                primitive: wgpu::PrimitiveState { topology, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let scene_pipeline = scene_pipeline_for(wgpu::PrimitiveTopology::TriangleList, wgpu::ColorWrites::ALL);
        let line_pipeline = scene_pipeline_for(wgpu::PrimitiveTopology::LineList, wgpu::ColorWrites::ALL);
        let occluder_pipeline = scene_pipeline_for(wgpu::PrimitiveTopology::TriangleList, wgpu::ColorWrites::empty());

        let warp_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let (tan_h, tan_v) = calibration.eye_tangents();
        let (ray_w, ray_h, ray_data) = ray_table(calibration);
        let rays = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("rays"),
                size: wgpu::Extent3d { width: ray_w, height: ray_h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&ray_data),
        );
        let warp = WarpUniform {
            reproject: Mat3::IDENTITY.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.0]),
            rect_sign: [1.0, 1.0],
            tan_half: [tan_h, tan_v],
            offset_left: [0.0; 2],
            offset_right: [0.0; 2],
            strength: 1.0,
            rotate: 0.0,
            _pad: [0.0; 2],
        };
        let warp_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("warp"),
            contents: bytemuck::bytes_of(&warp),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let eyes_view = eyes.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let warp_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &warp_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: warp_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&eyes_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&rays.create_view(&Default::default())) },
            ],
        });
        let warp_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("warp"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/warp.wgsl").into()),
        });
        let warp_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&warp_layout],
                push_constant_ranges: &[],
            })),
            vertex: wgpu::VertexState { module: &warp_shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &warp_shader, entry_point: Some("fs_main"), compilation_options: Default::default(), targets: &[Some(format.into())] }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let instance_capacity = 1024;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: instance_capacity * std::mem::size_of::<Instance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            cube: cube_mesh(&device),
            sphere: sphere_mesh(&device, 10, 16),
            line: line_mesh(&device),
            projection: Mat4::perspective_rh(2.0 * tan_v.atan(), tan_h / tan_v, NEAR, FAR),
            eye_position: calibration.eye_position,
            depth_view: depth.create_view(&Default::default()),
            surface,
            device,
            queue,
            config,
            scene_pipeline,
            line_pipeline,
            occluder_pipeline,
            warp_pipeline,
            camera_buffers,
            camera_groups,
            warp_buffer,
            warp_group,
            eyes_view,
            instances,
            instance_capacity,
            eye_size,
            warp,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Renders one frame. `build` runs only after the swapchain image is acquired —
    /// that call blocks on vsync, so sampling the pose before it would make every
    /// frame a full refresh interval stale.
    pub fn frame(&mut self, build: impl FnOnce() -> (Mat4, Vec<(Kind, Vec<Instance>)>, WarpSettings)) -> bool {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
            Err(e) => {
                log::warn!("surface: {e}");
                return false;
            }
        };
        let (rig, draws, warp) = build();
        let draws = &draws;
        let warp = &warp;

        for (i, buffer) in self.camera_buffers.iter().enumerate() {
            let view = (rig * Mat4::from_translation(self.eye_position[i])).inverse();
            let uniform = CameraUniform { view_proj: (self.projection * view).to_cols_array_2d() };
            self.queue.write_buffer(buffer, 0, bytemuck::bytes_of(&uniform));
        }
        let all: Vec<Instance> = draws.iter().flat_map(|(_, v)| v.iter().copied()).collect();
        let count = (all.len() as u64).min(self.instance_capacity) as usize;
        self.queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&all[..count]));

        self.warp.strength = warp.strength;
        self.warp.rotate = if warp.rotate { 1.0 } else { 0.0 };
        self.warp.rect_sign = warp.rect_sign;
        self.warp.reproject = warp.reproject.to_cols_array_2d().map(|c| [c[0], c[1], c[2], 0.0]);
        self.queue.write_buffer(&self.warp_buffer, 0, bytemuck::bytes_of(&self.warp));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("eyes"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.eyes_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let (w, h) = (self.eye_size.0 as f32, self.eye_size.1 as f32);
            for eye in 0..2 {
                pass.set_viewport(eye as f32 * w, 0.0, w, h, 0.0, 1.0);
                pass.set_scissor_rect(eye as u32 * self.eye_size.0, 0, self.eye_size.0, self.eye_size.1);
                pass.set_bind_group(0, &self.camera_groups[eye], &[]);
                pass.set_vertex_buffer(1, self.instances.slice(..));
                let mut first = 0u32;
                for (kind, instances) in draws {
                    let (mesh, pipeline) = match kind {
                        Kind::Cube => (&self.cube, &self.scene_pipeline),
                        Kind::Sphere => (&self.sphere, &self.scene_pipeline),
                        Kind::Line => (&self.line, &self.line_pipeline),
                        Kind::Occluder => (&self.cube, &self.occluder_pipeline),
                    };
                    let n = instances.len() as u32;
                    if first + n <= count as u32 {
                        pass.set_pipeline(pipeline);
                        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                        pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..mesh.index_count, 0, first..first + n);
                    }
                    first += n;
                }
            }
        }
        {
            let view = frame.texture.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("warp"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.warp_pipeline);
            pass.set_bind_group(0, &self.warp_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
        true
    }
}

/// Column vectors of a rotation, for composing instance transforms.
pub fn transform(position: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(scale, rotation, position)
}

/// A line segment from `a` to `b` as a scaled unit-line instance.
pub fn segment(a: Vec3, b: Vec3, color: Vec3) -> Instance {
    let d = b - a;
    let rotation = Quat::from_rotation_arc(Vec3::X, d.normalize_or_zero());
    Instance::new(transform(a, rotation, Vec3::new(d.length(), 1.0, 1.0)), color, false)
}

pub fn _unused(_: Vec4) {}
