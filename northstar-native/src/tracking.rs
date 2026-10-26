//! Tracker threads: XR50 pose straight from the device (~950 Hz, in-process) and
//! Ultraleap hands through LeapC. Each publishes its latest sample with the
//! instant it was received, so the renderer can measure pose age at present time.
use crate::leapc;
use crate::transforms::{xr50_to_three, Pose};
use glam::{Quat, Vec3};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct HeadSample {
    pub pose: Pose,
    pub confidence: f64,
    pub received: Instant,
}

#[derive(Clone, Debug)]
pub struct HandSample {
    pub left: bool,
    /// Palm orientation in the Leap frame
    pub palm_orientation: Quat,
    /// Leap frame, mm: palm then every bone joint (carp..tip per finger)
    pub joints: Vec<Vec3>,
}

#[derive(Default)]
pub struct Tracking {
    pub head: Mutex<Option<HeadSample>>,
    pub hands: Mutex<Vec<HandSample>>,
    pub head_rate: Mutex<u32>,
    /// Leap tracking events received, for diagnostics
    pub leap_frames: std::sync::atomic::AtomicU64,
}

pub type Shared = Arc<Tracking>;

/// Streams XR50 poses for the life of the process, reopening the device whenever
/// it goes away or stops answering (USB suspend, re-plug), like the bridge does.
pub fn spawn_xr50(shared: Shared) {
    std::thread::Builder::new()
        .name("xr50".into())
        .spawn(move || loop {
            let mut device = match xvisio::Device::open_first() {
                Ok(d) => d,
                Err(e) => {
                    log::warn!("XR50 open: {e}; retrying");
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };
            // A re-enumerated device can answer with an empty identity; SLAM from that
            // state streams garbage, so wait for it to recover (a re-plug clears it).
            if device.uuid().trim().is_empty() || device.features().is_empty() {
                log::warn!("XR50 degraded (empty identity) - re-plug the headset USB if this persists");
                std::thread::sleep(Duration::from_millis(1500));
                continue;
            }
            let stream = match device.start_slam(xvisio::SlamMode::Edge) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("XR50 start: {e}; retrying");
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };
            log::info!("XR50 {} streaming", device.uuid());
            let mut count = 0u32;
            let mut tick = Instant::now();
            let mut last_good = Instant::now();
            loop {
                let Ok(sample) = stream.recv_timeout(Duration::from_secs(2)) else {
                    log::warn!("XR50 stalled - reopening");
                    break;
                };
                let p = &sample.pose;
                let unit = (p.quaternion.iter().map(|v| v * v).sum::<f64>().sqrt() - 1.0).abs() < 0.1;
                let plausible = p.translation.iter().all(|v| v.abs() < 100.0);
                if unit && plausible {
                    last_good = Instant::now();
                    *shared.head.lock().unwrap() = Some(HeadSample {
                        pose: xr50_to_three(p.translation, p.quaternion),
                        confidence: p.confidence,
                        received: last_good,
                    });
                } else if last_good.elapsed() > Duration::from_secs(3) {
                    log::warn!("XR50 poses invalid for 3 s - reopening");
                    break;
                }
                count += 1;
                if tick.elapsed() >= Duration::from_secs(1) {
                    *shared.head_rate.lock().unwrap() = count;
                    count = 0;
                    tick = Instant::now();
                }
            }
            *shared.head_rate.lock().unwrap() = 0;
        })
        .expect("xr50 thread");
}

/// Streams XR50 poses from the WebSocket bridge (the single owner of the sensor)
/// at the full sample rate (`?rate=full`), reconnecting whenever it drops.
/// Pose age is measured from the bridge's wall-clock stamp (`tw`, ms).
pub fn spawn_xr50_client(shared: Shared, url: String) {
    #[derive(serde::Deserialize)]
    struct Msg { x: f64, y: f64, z: f64, q: [f64; 4], c: Option<f64>, tw: Option<f64> }
    let wall_ms = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1e3);
    std::thread::Builder::new()
        .name("xr50-client".into())
        .spawn(move || loop {
            let Ok((mut ws, _)) = tungstenite::connect(&url) else {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            };
            log::info!("XR50 bridge connected ({url})");
            let mut count = 0u32;
            let mut tick = Instant::now();
            while let Ok(message) = ws.read() {
                let Ok(text) = message.to_text() else { continue };
                let Ok(m) = serde_json::from_str::<Msg>(text) else { continue };
                let unit = (m.q.iter().map(|v| v * v).sum::<f64>().sqrt() - 1.0).abs() < 0.1;
                if unit && [m.x, m.y, m.z].iter().all(|v| v.abs() < 100.0) {
                    let age_ms = m.tw.map_or(0.0, |tw| (wall_ms() - tw).max(0.0));
                    *shared.head.lock().unwrap() = Some(HeadSample {
                        pose: xr50_to_three([m.x, m.y, m.z], m.q),
                        confidence: m.c.unwrap_or(1.0),
                        received: Instant::now() - Duration::from_secs_f64(age_ms / 1e3),
                    });
                }
                count += 1;
                if tick.elapsed() >= Duration::from_secs(1) {
                    *shared.head_rate.lock().unwrap() = count;
                    count = 0;
                    tick = Instant::now();
                }
            }
            log::warn!("XR50 bridge disconnected - reconnecting");
            *shared.head_rate.lock().unwrap() = 0;
        })
        .expect("xr50 client thread");
}

fn vec(v: leapc::Vector) -> Vec3 { Vec3::new(v.x, v.y, v.z) }

fn hand_sample(hand: &leapc::Hand) -> HandSample {
    // LeapC structs are packed: copy fields out before touching them.
    let (palm, arm, digits) = (hand.palm, hand.arm, hand.digits);
    let mut joints = vec![vec(palm.position), vec(arm.next_joint)];
    for digit in digits {
        let bones = digit.bones;
        joints.push(vec(bones[0].prev_joint));
        joints.extend(bones.iter().map(|b| vec(b.next_joint)));
    }
    let o = palm.orientation;
    HandSample { left: hand.hand_type == leapc::HAND_LEFT, palm_orientation: Quat::from_xyzw(o.x, o.y, o.z, o.w), joints }
}

/// Connects to the Ultraleap service and streams hands until the process exits.
pub fn spawn_leap(shared: Shared) -> Result<(), String> {
    let mut connection: leapc::Connection = std::ptr::null_mut();
    unsafe {
        if leapc::LeapCreateConnection(std::ptr::null(), &mut connection) != leapc::RS_SUCCESS {
            return Err("LeapCreateConnection failed".into());
        }
        if leapc::LeapOpenConnection(connection) != leapc::RS_SUCCESS {
            return Err("LeapOpenConnection failed".into());
        }
    }
    let connection = connection as usize; // raw pointers are not Send
    std::thread::Builder::new()
        .name("leap".into())
        .spawn(move || {
            let connection = connection as leapc::Connection;
            let mut msg = leapc::ConnectionMessage { size: 0, event_type: 0, pointer: std::ptr::null(), device_id: 0 };
            let mut connected = false;
            let mut seen = std::collections::HashSet::new();
            loop {
                let rs = unsafe { leapc::LeapPollConnection(connection, 1000, &mut msg) };
                if rs != leapc::RS_SUCCESS && rs != leapc::RS_TIMEOUT {
                    log::warn!("LeapPollConnection: {rs:#x}");
                    continue;
                }
                let event_type = { msg.event_type };
                if seen.insert(event_type) {
                    log::info!("LeapC event type {event_type:#x} (poll rs {rs:#x})");
                }
                match event_type {
                    leapc::EVENT_POLICY if !msg.pointer.is_null() => {
                        let flags = unsafe { *({ msg.pointer } as *const [u32; 2]) }[1];
                        log::info!("Ultraleap policy flags now {flags:#x}");
                    }
                    leapc::EVENT_CONNECTION_LOST => {
                        connected = false;
                        log::warn!("Ultraleap connection lost - waiting for the service");
                    }
                    leapc::EVENT_CONNECTION => unsafe {
                        connected = true;
                        // Frames only flow to the foreground app unless we ask for background frames.
                        let policy = leapc::LeapSetPolicyFlags(connection, leapc::POLICY_BACKGROUND_FRAMES, 0);
                        let mode = leapc::LeapSetTrackingMode(connection, leapc::TRACKING_MODE_HMD);
                        log::info!("Ultraleap connected (background frames {policy:#x}, HMD mode {mode:#x})");
                    },
                    leapc::EVENT_TRACKING => {
                        if !connected || { msg.pointer }.is_null() {
                            log::warn!("tracking event ignored (connected {connected}, pointer {:?})", { msg.pointer });
                            continue;
                        }
                        let event = unsafe { &*(msg.pointer as *const leapc::TrackingEvent) };
                        let n = { event.n_hands } as usize;
                        if shared.leap_frames.load(std::sync::atomic::Ordering::Relaxed) < 3 {
                            let (size, ptr, frame, at, rate) = ({ msg.size }, { msg.pointer }, { event.tracking_frame_id }, { event.hands }, { event.framerate });
                            log::info!("tracking event: msg size {size} ptr {ptr:?} frame {frame} hands {n} at {at:?} rate {rate}");
                        }
                        let hands_ptr = { event.hands };
                        if n > 4 || (n > 0 && hands_ptr.is_null()) {
                            log::warn!("LeapC tracking event with {n} hands at {hands_ptr:?}: layout mismatch, skipping");
                            continue;
                        }
                        let hands = unsafe { std::slice::from_raw_parts(hands_ptr, n) };
                        if n > 0 && shared.leap_frames.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
                            let h = hands[0];
                            let (p, frame) = (h.palm.position, event.tracking_frame_id);
                            log::info!("hand id={} type={} palm=({:.0},{:.0},{:.0}) pinch={:.2} frame={}", { h.id }, { h.hand_type }, { p.x }, { p.y }, { p.z }, { h.pinch_strength }, frame);
                        }
                        *shared.hands.lock().unwrap() = hands.iter().map(hand_sample).collect();
                        shared.leap_frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
        })
        .expect("leap thread");
    Ok(())
}
