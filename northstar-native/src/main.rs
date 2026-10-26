//! Native pass-through XR for Project NorthStar: XR50 pose in-process, Leap hands
//! through LeapC, wgpu stereo render + optics warp, vsync on the panel.
//!
//!   cargo run --release -- [--windowed] [--no-leap] [--xr50 bridge|direct|off] [--at X,Y] [--monitor N] [--low-power] [--calibration <path>]
//!
//! By default the head pose comes from the XR50 WebSocket bridge (which stays the one
//! owner of the sensor) at the full 950 Hz; `--xr50 direct` opens the device in-process.
//!
//! Hands: pinch a cube to move and turn it, pinch it with both hands to scale it,
//! press the floating buttons (warp, re-projection, reset) with a fingertip. The real
//! hands occlude the holograms. Keys: U toggles the warp, R the re-projection, X / Y
//! flip the warp axes, Esc quits. Prints pose age at present and frame pacing once a
//! second (compare with the `[XR] pose age` line the WebXR runtime logs).
mod calibration;
mod crash;
mod interact;
mod leapc;
mod render;
mod tracking;
mod transforms;

use calibration::Calibration;
use glam::{Mat3, Quat, Vec3};
use interact::{Action, Button, Interaction, Object};
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
    /// Panel rotation applied by Windows (the hub sets 180°); the swapchain ignores it, we compensate
    panel_rotation: u32,
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
        panel_rotation: value("--panel-rotation").and_then(|v| v.parse().ok()).unwrap_or(180),
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
    interaction: Interaction,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    warp: bool,
    reproject: bool,
    rect_sign: [f32; 2],
    stats: Stats,
}

/// The static part of the demo scene: grid and anchors at eye height.
fn scene() -> Vec<(Kind, Vec<Instance>)> {
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
    vec![(Kind::Line, grid.into_iter().chain(cross).collect()), (Kind::Sphere, anchors)]
}

/// Twenty cubes within reach, the same places the web app puts them.
fn cubes() -> Vec<Object> {
    (0..20)
        .map(|seed| {
            let s = seed as f32;
            Object {
                position: Vec3::new((s * 13.37) % 4.0 - 2.0, (s * 7.91) % 2.0 + 0.5, (s * 3.53) % 4.0 - 2.0),
                rotation: Quat::from_rotation_y(s * 0.7),
                scale: 0.1,
                color: Vec3::new(0.0, 1.0, 1.0),
            }
        })
        .collect()
}

/// One reset button, low and to the side so a passing hand does not hit it. (Warp and
/// re-projection stay on the keyboard: a toggle in reach of the hands got pressed by accident
/// and switched the optics off.)
fn buttons() -> Vec<Button> {
    vec![Button::new(Vec3::new(0.45, 1.0, -0.6), Vec3::new(0.8, 0.25, 0.25), Action::Reset)]
}

impl App {
    fn head(&self) -> (Pose, Option<Instant>, f64) {
        match *self.tracking.head.lock().unwrap() {
            Some(s) => (s.pose, Some(s.received), s.confidence),
            None => (Pose::default(), None, 0.0),
        }
    }

    fn render(&mut self) {
        let Some(mut renderer) = self.renderer.take() else { return };
        let mut received = None;
        let mut actions = Vec::new();
        let mut hand_count = 0;
        let presented = renderer.frame(|| {
            let (pose, at, _confidence) = self.head();
            received = at;
            let rig = rig(&pose);
            let samples = self.tracking.hands.lock().unwrap().clone();
            let hands = interact::hands(&samples, &self.leap_to_head, rig);
            hand_count = hands.iter().flatten().count();
            actions = self.interaction.update(&hands);
            let mut draws = self.interaction.draws(&hands, |_| false);
            draws.extend(scene());
            // Freshest pose for the warp: with ~1 ms pose age this is late latching.
            let display = self.head().0.rotation;
            let warp = WarpSettings {
                strength: if self.warp { 1.0 } else { 0.0 },
                rotate: self.options.panel_rotation == 180 && !self.options.windowed,
                rect_sign: self.rect_sign,
                reproject: if self.reproject { reprojection(pose.rotation, display) } else { Mat3::IDENTITY },
            };
            (rig, draws, warp)
        });
        self.renderer = Some(renderer);
        for action in actions {
            log::info!("button: {action:?}");
            match action {
                Action::Reset => self.interaction.objects = cubes(),
            }
        }
        if !presented {
            return;
        }
        let rate = *self.tracking.head_rate.lock().unwrap();
        let leap_frames = self.tracking.leap_frames.load(std::sync::atomic::Ordering::Relaxed);
        if self.stats.tick.is_none() {
            log::info!("first frame presented");
        }
        self.stats.present(received, rate, hand_count, leap_frames);
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
        interaction: Interaction::new(cubes(), buttons()),
        window: None,
        renderer: None,
        warp: true,
        reproject: true,
        // Optics-only signs; tracking orientation is fixed in transforms.rs and by the panel rotation.
        rect_sign: [1.0, 1.0],
        stats: Stats::default(),
    };
    EventLoop::new().expect("event loop").run_app(&mut app).expect("run");
}
