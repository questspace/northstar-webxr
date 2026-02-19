use crate::protocol;
use crate::types::SlamSample;
use crate::{Result, XvisioError};
use crossbeam_channel::{Receiver, Sender};
use hidapi::HidDevice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Handle to an active SLAM data stream.
///
/// Receives ~950 Hz pose data from a background reader thread that
/// reads HID interrupt reports via hidapi.
pub struct SlamStream {
    receiver: Receiver<SlamSample>,
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SlamStream {
    /// Start the SLAM streaming thread.
    ///
    /// The reader thread owns the HidDevice and calls read_timeout() in a loop.
    /// hidapi handles OS-level buffering of interrupt IN reports.
    pub(crate) fn start(device: HidDevice) -> Result<SlamStream> {
        let (sender, receiver) = crossbeam_channel::bounded(256);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();

        let thread = std::thread::Builder::new()
            .name("xvisio-slam".into())
            .spawn(move || {
                slam_reader_loop(device, sender, stop_clone);
            })
            .map_err(|e| XvisioError::HidCommand(format!("Failed to spawn SLAM thread: {}", e)))?;

        Ok(SlamStream {
            receiver,
            stop_flag,
            thread: Some(thread),
        })
    }

    /// Receive the next SLAM sample (blocks until available).
    pub fn recv(&self) -> Result<SlamSample> {
        self.receiver
            .recv()
            .map_err(|_| XvisioError::StreamStopped)
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

/// The SLAM reader loop runs in a dedicated thread.
///
/// Reads HID interrupt IN reports via hidapi's read_timeout().
/// Each report is 64 bytes (1 byte report ID + 63 bytes data).
/// hidapi handles OS-level double-buffering internally.
fn slam_reader_loop(device: HidDevice, sender: Sender<SlamSample>, stop_flag: Arc<AtomicBool>) {
    let epoch = Instant::now();
    let mut buf = [0u8; 64];

    log::info!("SLAM reader started (hidapi)");

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            log::info!("SLAM reader stopping (stop flag set)");
            break;
        }

        // read_timeout: 100ms to periodically check stop flag
        let len = match device.read_timeout(&mut buf, 100) {
            Ok(0) => continue,      // timeout, no data
            Ok(n) => n,
            Err(e) => {
                log::warn!("SLAM read error: {}", e);
                continue;
            }
        };

        // hidapi on Windows may or may not prepend report ID.
        // If first byte is 0x01 (our SLAM_HEADER[0]) and we got enough data, parse directly.
        // Otherwise, if we got exactly REPORT_SIZE bytes without report ID, prepend it.
        let data: &[u8] = if len >= protocol::REPORT_SIZE && buf[0] == protocol::SLAM_HEADER[0] {
            &buf[..len]
        } else {
            // Not a SLAM packet (could be a control response), skip
            continue;
        };

        if let Some(sample) = protocol::parse_slam_packet(data, epoch) {
            if let Err(e) = sender.try_send(sample) {
                match e {
                    crossbeam_channel::TrySendError::Full(_) => {
                        log::trace!("SLAM channel full, dropping sample");
                    }
                    crossbeam_channel::TrySendError::Disconnected(_) => {
                        log::info!("SLAM channel disconnected, stopping reader");
                        break;
                    }
                }
            }
        }
    }
}
