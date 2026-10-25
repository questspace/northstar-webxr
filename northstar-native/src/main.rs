//! Native pass-through XR for Project NorthStar: XR50 pose in-process, Leap hands
//! through LeapC, wgpu stereo render + optics warp, vsync on the panel.
//!
//!   cargo run --release -- [--windowed] [--no-leap] [--xr50 bridge|direct|off] [--at X,Y] [--monitor N] [--low-power] [--calibration <path>]
//!
//! By default the head pose comes from the XR50 WebSocket bridge (which stays the one
//! owner of the sensor) at the full 950 Hz; `--xr50 direct` opens the device in-process.
//!
//! Keys: U toggles the warp, R the re-projection, X / Y flip the warp axes, Esc quits.
//! Prints pose age at present and frame pacing once a second (compare with the
//! `[XR] pose age` line the WebXR runtime logs in the browser console).
mod calibration;
mod crash;
mod leapc;
mod render;
mod tracking;
mod transforms;

use calibration::Calibration;
use glam::{Mat3, Quat, Vec3};
use render::{segment, transform, Instance, Kind, Renderer, WarpSettings};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracking::Shared;
use transforms::{reprojection, rig, LeapToHead, Pose, EYE_HEIGHT};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

struct Options {
    windowed: bool,
    leap: bool,
    /// "bridge" (default: client of the WebSocket bridge, which owns the sensor),
    /// "direct" (open the XR50 in-process) or "off"
    xr50: String,
    monitor: Option<usize>,
    /// Desktop position of the panel (the hub passes this)
    at: Option<(i32, i32)>,
    /// Render on the integrated GPU (use when the panel is wired to it)
    low_power: bool,
    calibration: String,
}

fn options() -> Options {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned());
    Options {
        windowed: args.iter().any(|a| a == "--windowed"),
        leap: !args.iter().any(|a| a == "--no-leap"),
        xr50: if args.iter().any(|a| a == "--no-xr50") { "off".into() } else { value("--xr50").unwrap_or_else(|| "bridge".into()) },
        monitor: value("--monitor").and_then(|v| v.parse().ok()),
        at: value("--at").and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        }),
        low_power: args.iter().any(|a| a == "--low-power"),
        calibration: value("--calibration")
            .unwrap_or_else(|| "../visual-test/public/calibration/EskySettings.json".into()),
    }
}

#[derive(Default)]
struct Stats {
    frames: u32,
    pose_age_sum: f64,
    pose_age_max: f64,
    interval_max: f64,
    last_present: Option<Instant>,
    tick: Option<Instant>,
}

impl Stats {
    fn present(&mut self, pose_received: Option<Instant>, head_rate: u32, hands: usize, leap_frames: u64) {
        let now = Instant::now();
        if let Some(last) = self.last_present {
            self.interval_max = self.interval_max.max((now - last).as_secs_f64() * 1e3);
        }
        self.last_present = Some(now);
        let age = pose_received.map(|t| (now - t).as_secs_f64() * 1e3).unwrap_or(0.0);
        self.pose_age_sum += age;
        self.pose_age_max = self.pose_age_max.max(age);
        self.frames += 1;
        let tick = *self.tick.get_or_insert(now);
        if now - tick >= Duration::from_secs(1) {
            let n = self.frames.max(1) as f64;
            println!(
                "[native] {:.0} fps  frame max {:.1} ms  pose age avg {:.2} ms max {:.2} ms  xr50 {} Hz  hands {}  leap frames {}",
                n / (now - tick).as_secs_f64(),
                self.interval_max,
                self.pose_age_sum / n,
                self.pose_age_max,
                head_rate,
                hands,
                leap_frames,
            );
            // stdout is block-buffered when redirected to a log file
            let _ = std::io::Write::flush(&mut std::io::stdout());
            *self = Stats { last_present: Some(now), tick: Some(now), ..Default::default() };
        }
    }
}

struct App {
    options: Options,
    calibration: Calibration,
    tracking: Shared,
    leap_to_head: LeapToHead,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    warp: bool,
    reproject: bool,
    rect_sign: [f32; 2],
    stats: Stats,
    start: Instant,
}

/// The same demo scene as the web app: grid, anchors at eye height, floating cubes.
fn scene(t: f32) -> Vec<(Kind, Vec<Instance>)> {
    let grid: Vec<Instance> = (0..=40)
        .flat_map(|i| {
            let p = i as f32 * 0.5 - 10.0;
            let c = if i % 5 == 0 { Vec3::new(0.27, 0.27, 0.4) } else { Vec3::new(0.2, 0.2, 0.33) };
            [segment(Vec3::new(p, 0.0, -10.0), Vec3::new(p, 0.0, 10.0), c), segment(Vec3::new(-10.0, 0.0, p), Vec3::new(10.0, 0.0, p), c)]
        })
        .collect();
    let anchors = vec![
        Instance::new(transform(Vec3::new(0.0, 1.6, -0.5), Quat::IDENTITY, Vec3::splat(0.035)), Vec3::new(1.0, 0.27, 0.27), true),
        Instance::new(transform(Vec3::new(-0.18, 1.6, -1.0), Quat::IDENTITY, Vec3::splat(0.05)), Vec3::new(0.0, 1.0, 0.53), true),
        Instance::new(transform(Vec3::new(0.18, 1.6, -1.0), Quat::IDENTITY, Vec3::splat(0.05)), Vec3::new(0.4, 0.8, 1.0), true),
        Instance::new(transform(Vec3::new(0.0, 1.6, -1.8), Quat::IDENTITY, Vec3::splat(0.075)), Vec3::ONE, true),
    ];
    let cross = [1.0f32, -1.0].map(|s| {
        let d = Vec3::new(0.18, 0.18 * s, 0.0);
        segment(Vec3::new(0.0, 1.6, -1.0) - d, Vec3::new(0.0, 1.6, -1.0) + d, Vec3::ONE)
    });
    let cubes = (0..20)
        .map(|seed| {
            let s = seed as f32;
            let speed = 0.005 + ((s * 7.31) % 1.0) * 0.02;
            let angle = t * speed * 60.0;
            let position = Vec3::new((s * 13.37) % 4.0 - 2.0, (s * 7.91) % 2.0 + 0.5, (s * 3.53) % 4.0 - 2.0);
            let rotation = Quat::from_rotation_y(angle) * Quat::from_rotation_x(angle * 0.5);
            Instance::new(transform(position, rotation, Vec3::splat(0.1)), Vec3::new(0.0, 1.0, 1.0), true)
        })
        .collect();
    vec![(Kind::Line, grid.into_iter().chain(cross).collect()), (Kind::Sphere, anchors), (Kind::Cube, cubes)]
}

impl App {
    fn head(&self) -> (Pose, Option<Instant>, f64) {
        match *self.tracking.head.lock().unwrap() {
            Some(s) => (s.pose, Some(s.received), s.confidence),
            None => (Pose::default(), None, 0.0),
        }
    }

    fn hands(&self, rig: glam::Mat4) -> Vec<Instance> {
        self.tracking
            .hands
            .lock()
            .unwrap()
            .iter()
            .flat_map(|hand| {
                let color = if hand.left { Vec3::new(0.0, 1.0, 0.53) } else { Vec3::new(1.0, 0.4, 0.0) };
                hand.joints
                    .iter()
                    .enumerate()
                    .map(|(i, mm)| {
                        let p = rig.transform_point3(self.leap_to_head.point(*mm));
                        Instance::new(transform(p, Quat::IDENTITY, Vec3::splat(if i == 0 { 0.012 } else { 0.006 })), color, true)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn render(&mut self) {
        let Some(mut renderer) = self.renderer.take() else { return };
        let mut received = None;
        let presented = renderer.frame(|| {
            let (pose, at, _confidence) = self.head();
            received = at;
            let rig = rig(&pose);
            let mut draws = scene(self.start.elapsed().as_secs_f32());
            draws.push((Kind::Sphere, self.hands(rig)));
            // Freshest pose for the warp: with ~1 ms pose age this is late latching.
            let display = self.head().0.rotation;
            let warp = WarpSettings {
                strength: if self.warp { 1.0 } else { 0.0 },
                rect_sign: self.rect_sign,
                reproject: if self.reproject { reprojection(pose.rotation, display) } else { Mat3::IDENTITY },
            };
            (rig, draws, warp)
        });
        self.renderer = Some(renderer);
        if !presented {
            return;
        }
        let hands = self.tracking.hands.lock().unwrap().len();
        let rate = *self.tracking.head_rate.lock().unwrap();
        let leap_frames = self.tracking.leap_frames.load(std::sync::atomic::Ordering::Relaxed);
        if self.stats.tick.is_none() {
            log::info!("first frame presented");
        }
        self.stats.present(received, rate, hands, leap_frames);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let d = &self.calibration.display;
        let mut attributes = Window::default_attributes().with_title("NorthStar native");
        if self.options.windowed {
            attributes = attributes.with_inner_size(PhysicalSize::new(1440, 800));
        } else {
            // The panel: `--at x,y` (the hub knows where it is), `--monitor N`, else the
            // monitor running the calibration's native mode, else the non-primary one.
            // (Mode *support* is no discriminator: the laptop advertises a scaled
            // 2880x1600 too.) Borderless fullscreen there.
            let size = PhysicalSize::new(d.width, d.height);
            let monitors: Vec<_> = event_loop.available_monitors().collect();
            let primary = event_loop.primary_monitor();
            for (i, m) in monitors.iter().enumerate() {
                log::info!("monitor {i} {:?}: {}x{} at {:?}{}", m.name(), m.size().width, m.size().height, m.position(),
                    if Some(m) == primary.as_ref() { " (primary)" } else { "" });
            }
            let monitor = self.options.at
                .and_then(|(x, y)| monitors.iter().find(|m| m.position() == PhysicalPosition::new(x, y)))
                .or_else(|| self.options.monitor.and_then(|i| monitors.get(i)))
                .or_else(|| monitors.iter().find(|m| m.size() == size))
                .or_else(|| monitors.iter().find(|m| Some(*m) != primary.as_ref()))
                .cloned();
            attributes = match monitor {
                Some(m) => {
                    log::info!("panel: {:?} ({}x{} at {:?})", m.name(), m.size().width, m.size().height, m.position());
                    attributes.with_fullscreen(Some(Fullscreen::Borderless(Some(m))))
                }
                None => attributes
                    .with_decorations(false)
                    .with_position(PhysicalPosition::new(d.x, d.y))
                    .with_inner_size(PhysicalSize::new(d.width, d.height)),
            };
        }
        let window = Arc::new(event_loop.create_window(attributes).expect("window"));
        self.renderer = Some(Renderer::new(window.clone(), &self.calibration, self.options.low_power));
        self.window = Some(window);
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key, state: ElementState::Pressed, .. }, .. } => {
                match logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Character(c) => match c.to_lowercase().as_str() {
                        "u" => self.warp = !self.warp,
                        "r" => self.reproject = !self.reproject,
                        "x" => self.rect_sign[0] = -self.rect_sign[0],
                        "y" => self.rect_sign[1] = -self.rect_sign[1],
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("northstar_native=info,xvisio=warn,wgpu_hal=warn,wgpu_core=warn,naga=warn"),
    )
    .init();
    crash::install();
    let options = options();
    let calibration = Calibration::load(&options.calibration).unwrap_or_else(|e| {
        eprintln!("calibration {}: {e}", options.calibration);
        std::process::exit(1);
    });
    let tracking: Shared = Arc::new(tracking::Tracking::default());
    match options.xr50.as_str() {
        "direct" => tracking::spawn_xr50(tracking.clone()),
        "off" => log::info!("XR50 disabled: identity pose at {EYE_HEIGHT} m"),
        _ => tracking::spawn_xr50_client(tracking.clone(), "ws://localhost:8080/?rate=full".into()),
    }
    if options.leap {
        if let Err(e) = tracking::spawn_leap(tracking.clone()) {
            log::error!("Ultraleap: {e}");
        }
    }
    let leap_to_head = LeapToHead::new(&calibration);
    let mut app = App {
        options,
        calibration,
        tracking,
        leap_to_head,
        window: None,
        renderer: None,
        warp: true,
        reproject: true,
        // Verified on the headset: a negative vertical sign puts the floor above the anchors.
        rect_sign: [1.0, 1.0],
        stats: Stats::default(),
        start: Instant::now(),
    };
    EventLoop::new().expect("event loop").run_app(&mut app).expect("run");
}
