use crate::protocol;
use crate::types::SlamSample;
use crate::{Result, XvisioError};
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Handle to an active SLAM data stream.
///
/// Receives ~950 Hz pose data from a background reader thread that
/// reads HID interrupt reports via hidapi (Windows/Linux) or rusb (macOS).
pub struct SlamStream {
    receiver: Receiver<SlamSample>,
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Prevents hid_exit() on macOS while the reader thread is using the HidDevice.
    /// Only used when the hidapi backend is active (Windows/Linux).
    _api: Option<hidapi::HidApi>,
}

impl SlamStream {
    /// Start the SLAM streaming thread using hidapi (Windows/Linux).
    pub(crate) fn start_hidapi(
        device: hidapi::HidDevice,
        api: hidapi::HidApi,
    ) -> Result<SlamStream> {
        let (sender, receiver) = crossbeam_channel::bounded(256);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();

        let thread = std::thread::Builder::new()
            .name("xvisio-slam".into())
            .spawn(move || {
                slam_reader_hidapi(device, sender, stop_clone);
            })
            .map_err(|e| XvisioError::HidCommand(format!("Failed to spawn SLAM thread: {}", e)))?;

        Ok(SlamStream {
            receiver,
            stop_flag,
            thread: Some(thread),
            _api: Some(api),
        })
    }

    /// Start the SLAM streaming thread using rusb (macOS).
    pub(crate) fn start_rusb(
        handle: rusb::DeviceHandle<rusb::GlobalContext>,
    ) -> Result<SlamStream> {
        let (sender, receiver) = crossbeam_channel::bounded(256);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();

        let thread = std::thread::Builder::new()
            .name("xvisio-slam".into())
            .spawn(move || {
                slam_reader_rusb(handle, sender, stop_clone);
            })
            .map_err(|e| XvisioError::HidCommand(format!("Failed to spawn SLAM thread: {}", e)))?;

        Ok(SlamStream {
            receiver,
            stop_flag,
            thread: Some(thread),
            _api: None,
        })
    }

    /// Receive the next SLAM sample (blocks until available).
    pub fn recv(&self) -> Result<SlamSample> {
        self.receiver.recv().map_err(|_| XvisioError::StreamStopped)
    }

    /// Try to receive a SLAM sample without blocking.
    pub fn try_recv(&self) -> Option<SlamSample> {
        self.receiver.try_recv().ok()
    }

    /// Receive a SLAM sample with a timeout.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<SlamSample> {
        self.receiver.recv_timeout(timeout).map_err(|e| match e {
            crossbeam_channel::RecvTimeoutError::Timeout => XvisioError::Timeout,
            crossbeam_channel::RecvTimeoutError::Disconnected => XvisioError::StreamStopped,
        })
    }

    /// Check if the stream is still active.
    pub fn is_active(&self) -> bool {
        !self.stop_flag.load(Ordering::Relaxed)
    }

    /// Stop the stream and wait for the reader thread to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SlamStream {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// hidapi-based SLAM reader (Windows/Linux).
fn slam_reader_hidapi(
    device: hidapi::HidDevice,
    sender: Sender<SlamSample>,
    stop_flag: Arc<AtomicBool>,
) {
    let epoch = Instant::now();
    let mut buf = [0u8; 64];
    let debug_raw = std::env::var("XVISIO_DEBUG_RAW")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    let mut debug_packets: u32 = 0;

    log::info!("SLAM reader started (hidapi)");

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            log::info!("SLAM reader stopping (stop flag set)");
            break;
        }

        let len = match device.read_timeout(&mut buf, 100) {
            Ok(0) => continue,
            Ok(n) => n,
            Err(e) => {
                log::warn!("SLAM read error: {}", e);
                continue;
            }
        };

        let data: &[u8] = if len >= protocol::REPORT_SIZE && buf[0] == protocol::SLAM_HEADER[0] {
            &buf[..len]
        } else {
            if debug_raw && debug_packets < 20 {
                debug_packets += 1;
                let b0 = if len > 0 { buf[0] } else { 0 };
                let b1 = if len > 1 { buf[1] } else { 0 };
                let b2 = if len > 2 { buf[2] } else { 0 };
                log::info!(
                    "SLAM raw[{}]: len={} unexpected hdr={:02x} {:02x} {:02x}",
                    debug_packets,
                    len,
                    b0,
                    b1,
                    b2
                );
            }
            continue;
        };

        if debug_raw && debug_packets < 20 {
            debug_packets += 1;
            log::info!(
                "SLAM raw[{}]: len={} hdr={:02x} {:02x} {:02x} ts={:02x}{:02x}{:02x}{:02x}",
                debug_packets,
                len,
                data[0],
                data[1],
                data[2],
                data[6],
                data[5],
                data[4],
                data[3]
            );
        }
        dispatch_sample(data, epoch, &sender, &stop_flag);
    }
}

/// rusb-based SLAM reader (macOS).
fn slam_reader_rusb(
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    sender: Sender<SlamSample>,
    stop_flag: Arc<AtomicBool>,
) {
    let epoch = Instant::now();
    let mut buf = [0u8; 64];
    let timeout = Duration::from_millis(200);
    let mut consecutive_errors: u32 = 0;
    let debug_raw = std::env::var("XVISIO_DEBUG_RAW")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    let mut debug_packets: u32 = 0;

    log::info!("SLAM reader started (rusb)");

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            log::info!("SLAM reader stopping (stop flag set)");
            break;
        }

        let len = match handle.read_interrupt(protocol::SLAM_ENDPOINT, &mut buf, timeout) {
            Ok(n) => {
                consecutive_errors = 0;
                n
            }
            Err(rusb::Error::Timeout) => continue,
            Err(rusb::Error::NoDevice) => {
                log::error!("SLAM reader: device disconnected");
                stop_flag.store(true, Ordering::Relaxed);
                break;
            }
            Err(rusb::Error::Pipe) | Err(rusb::Error::Io) => {
                consecutive_errors += 1;
                if consecutive_errors <= 5 || consecutive_errors % 50 == 0 {
                    log::warn!("SLAM interrupt read recovery ({})", consecutive_errors);
                }
                handle.clear_halt(protocol::SLAM_ENDPOINT).ok();
                std::thread::sleep(Duration::from_millis(10));
                if consecutive_errors > 1000 {
                    log::error!("SLAM reader: too many recoverable errors, stopping");
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
                continue;
            }
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors <= 5 || consecutive_errors % 50 == 0 {
                    log::warn!("SLAM interrupt read error: {}", e);
                }
                std::thread::sleep(Duration::from_millis(10));
                if consecutive_errors > 1000 {
                    log::error!("SLAM reader: too many consecutive errors, stopping");
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
                continue;
            }
        };

        // Interrupt transfers don't include the report ID — the data starts
        // directly with the command echo bytes (0xA2, 0x33).
        // Prepend the report ID (0x01) to match the expected SLAM packet format.
        if len >= 2 && buf[0] == protocol::SLAM_HEADER[1] && buf[1] == protocol::SLAM_HEADER[2] {
            // Shift data right by 1 and insert report ID
            let total = (len + 1).min(64);
            buf.copy_within(0..len, 1);
            buf[0] = protocol::SLAM_HEADER[0]; // 0x01
            if debug_raw && debug_packets < 20 {
                debug_packets += 1;
                log::info!(
                    "SLAM raw[{}]: len={} hdr={:02x} {:02x} {:02x}",
                    debug_packets,
                    total,
                    buf[0],
                    buf[1],
                    buf[2]
                );
            }
            dispatch_sample(&buf[..total], epoch, &sender, &stop_flag);
        } else if len >= protocol::REPORT_SIZE && buf[0] == protocol::SLAM_HEADER[0] {
            // Report ID is included (some libusb configurations)
            if debug_raw && debug_packets < 20 {
                debug_packets += 1;
                log::info!(
                    "SLAM raw[{}]: len={} hdr={:02x} {:02x} {:02x}",
                    debug_packets,
                    len,
                    buf[0],
                    buf[1],
                    buf[2]
                );
            }
            dispatch_sample(&buf[..len], epoch, &sender, &stop_flag);
        } else if debug_raw && debug_packets < 20 {
            debug_packets += 1;
            let b0 = if len > 0 { buf[0] } else { 0 };
            let b1 = if len > 1 { buf[1] } else { 0 };
            let b2 = if len > 2 { buf[2] } else { 0 };
            log::info!(
                "SLAM raw[{}]: len={} unexpected hdr={:02x} {:02x} {:02x}",
                debug_packets,
                len,
                b0,
                b1,
                b2
            );
        }
    }

    // Release interface — ignore errors (device may already be disconnected)
    handle.release_interface(protocol::HID_INTERFACE as u8).ok();
    log::info!("SLAM reader stopped");
}

/// Parse and send a SLAM sample to the channel.
fn dispatch_sample(
    data: &[u8],
    epoch: Instant,
    sender: &Sender<SlamSample>,
    stop_flag: &Arc<AtomicBool>,
) {
    if let Some(sample) = protocol::parse_slam_packet(data, epoch) {
        if let Err(e) = sender.try_send(sample) {
            match e {
                crossbeam_channel::TrySendError::Full(_) => {
                    log::trace!("SLAM channel full, dropping sample");
                }
                crossbeam_channel::TrySendError::Disconnected(_) => {
                    log::info!("SLAM channel disconnected, stopping reader");
                    stop_flag.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}
