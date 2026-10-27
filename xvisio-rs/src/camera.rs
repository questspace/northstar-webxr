//! The XR50's stereo fisheye cameras (feature `camera`).
//!
//! The cameras are a standard UVC device ("XVisio vSLAM"), read through the operating
//! system's camera API via `nokhwa` (Media Foundation, V4L2, AVFoundation), so no driver
//! changes are needed. Each frame is 640x800 NV12 at 100 fps with the two cameras stacked;
//! only the luma plane is used. The device sends images only after
//! [`crate::Device::start_slam_with_cameras`].
//!
//! A reader thread drains the stream at the camera's rate and keeps only the newest frame, so
//! a slower consumer (depth runs at ~25 fps) always gets a fresh image instead of the oldest
//! one queued by the OS (which measured ~180 ms stale).
use crate::{Result, XvisioError};
use nokhwa::utils::{ApiBackend, CameraFormat, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// One stereo pair, 8-bit grey.
pub struct StereoFrame {
    /// Width and height of each image (640 x 400).
    pub width: u32,
    pub height: u32,
    /// Top and bottom half of the sensor frame, in that order (top = calibration camera 0).
    pub images: [Vec<u8>; 2],
    /// When the OS handed the frame over.
    pub received: Instant,
}

/// Newest frame (taken by the consumer) or the reader's error.
type Latest = (Mutex<Option<Result<StereoFrame>>>, Condvar);

/// The XR50's stereo fisheye camera stream.
pub struct StereoCamera {
    latest: Arc<Latest>,
    stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
}

const NAME: &str = "XVisio vSLAM";
const WIDTH: u32 = 640;
const HEIGHT: u32 = 800;
const FPS: u32 = 100;
const EYE: usize = (WIDTH * HEIGHT / 2) as usize;

fn err(e: impl std::fmt::Display) -> XvisioError {
    XvisioError::Camera(e.to_string())
}

fn open_camera() -> Result<nokhwa::Camera> {
    let info = nokhwa::query(ApiBackend::Auto)
        .map_err(err)?
        .into_iter()
        .find(|c| c.human_name().contains(NAME))
        .ok_or_else(|| XvisioError::Camera(format!("no camera named \"{NAME}\"")))?;
    let format = CameraFormat::new(Resolution::new(WIDTH, HEIGHT), FrameFormat::NV12, FPS);
    let requested = RequestedFormat::new::<nokhwa::pixel_format::LumaFormat>(RequestedFormatType::Exact(format));
    let mut camera = nokhwa::Camera::new(info.index().clone(), requested).map_err(err)?;
    camera.open_stream().map_err(err)?;
    Ok(camera)
}

/// NV12 starts with the full-resolution luma plane: the two stacked camera images.
fn split(raw: &[u8], received: Instant) -> Result<StereoFrame> {
    if raw.len() < 2 * EYE {
        return Err(XvisioError::Camera(format!("short frame: {} bytes", raw.len())));
    }
    Ok(StereoFrame { width: WIDTH, height: HEIGHT / 2, images: [raw[..EYE].to_vec(), raw[EYE..2 * EYE].to_vec()], received })
}

impl StereoCamera {
    /// Opens the XR50's camera (640x800 NV12 @ 100 fps) and starts streaming.
    pub fn open() -> Result<StereoCamera> {
        let latest: Arc<Latest> = Arc::new((Mutex::new(None), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (opened_tx, opened_rx) = mpsc::channel();
        let (slot, quit) = (latest.clone(), stop.clone());
        // The camera lives on its own thread (Media Foundation objects are not Send).
        let reader = std::thread::spawn(move || {
            let mut camera = match open_camera() {
                Ok(c) => c,
                Err(e) => return drop(opened_tx.send(Err(e))),
            };
            let _ = opened_tx.send(Ok(()));
            while !quit.load(Ordering::Relaxed) {
                let frame = camera.frame_raw().map_err(err).and_then(|raw| split(&raw, Instant::now()));
                let failed = frame.is_err();
                *slot.0.lock().unwrap() = Some(frame);
                slot.1.notify_all();
                if failed {
                    break;
                }
            }
            let _ = camera.stop_stream();
        });
        opened_rx.recv().map_err(err)??;
        Ok(StereoCamera { latest, stop, reader: Some(reader) })
    }

    /// The newest frame not returned before (blocks until one arrives).
    pub fn frame(&mut self) -> Result<StereoFrame> {
        let (slot, ready) = &*self.latest;
        let mut guard = slot.lock().unwrap();
        loop {
            if let Some(frame) = guard.take() {
                return frame;
            }
            let (g, timeout) = ready.wait_timeout(guard, Duration::from_secs(2)).unwrap();
            if timeout.timed_out() && g.is_none() {
                return Err(XvisioError::Camera("no frame for 2 s".into()));
            }
            guard = g;
        }
    }
}

impl Drop for StereoCamera {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
