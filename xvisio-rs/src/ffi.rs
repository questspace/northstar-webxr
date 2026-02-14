//! C FFI layer for xvisio.
//!
//! Provides opaque handle-based API for C/C++ consumers.
//! The generated C header is written to `include/xvisio.h` by cbindgen.

use crate::device::Device;
use crate::error::LastError;
use crate::slam::SlamStream;
use crate::types::SlamMode;
use std::ffi::{c_char, c_int};
use std::time::Duration;

/// Thread-local last error message for C consumers.
static LAST_ERROR: LastError = LastError::new();

/// Opaque device handle for C consumers.
pub struct XvDevice(Device);

/// Opaque SLAM stream handle for C consumers.
pub struct XvSlamStream(SlamStream);

/// Pose data in C-compatible layout.
#[repr(C)]
pub struct XvPose {
    /// Translation [x, y, z] in meters.
    pub translation: [f64; 3],
    /// Rotation matrix, flat row-major (9 elements).
    pub rotation: [f64; 9],
    /// Quaternion [qx, qy, qz, qw].
    pub quaternion: [f64; 4],
    /// Edge timestamp in microseconds.
    pub timestamp_us: u64,
    /// Host steady-clock timestamp in seconds.
    pub host_timestamp_s: f64,
    /// Tracking confidence [0..1].
    pub confidence: f64,
    /// Euler angles [roll, pitch, yaw] in degrees.
    pub euler_deg: [f64; 3],
}

/// Device info in C-compatible layout.
#[repr(C)]
pub struct XvDeviceInfo {
    /// Null-terminated UUID string.
    pub uuid: [c_char; 64],
    /// Null-terminated version string.
    pub version: [c_char; 128],
    /// Feature bitmap.
    pub features: u32,
    /// USB bus identifier (first 32 chars).
    pub bus_id: [c_char; 32],
    /// USB device address.
    pub address: u8,
}

fn str_to_fixed<const N: usize>(s: &str) -> [c_char; N] {
    let mut buf = [0 as c_char; N];
    let bytes = s.as_bytes();
    let len = bytes.len().min(N - 1);
    for (i, &b) in bytes[..len].iter().enumerate() {
        buf[i] = b as c_char;
    }
    buf
}

/// List connected XR50 devices.
///
/// Writes up to `max` entries into `out`. Returns the number of devices found,
/// or -1 on error.
///
/// # Safety
/// `out` must point to an array of at least `max` `XvDeviceInfo` elements, or be null.
#[no_mangle]
pub unsafe extern "C" fn xv_list_devices(out: *mut XvDeviceInfo, max: c_int) -> c_int {
    match crate::device::list_devices() {
        Ok(devices) => {
            let count = devices.len().min(max as usize);
            if !out.is_null() {
                for (i, dev) in devices.iter().take(count).enumerate() {
                    let info = XvDeviceInfo {
                        uuid: str_to_fixed(&dev.uuid),
                        version: str_to_fixed(&dev.version),
                        features: dev.features.bits(),
                        bus_id: str_to_fixed(&dev.bus_id),
                        address: dev.device_address,
                    };
                    out.add(i).write(info);
                }
            }
            count as c_int
        }
        Err(e) => {
            LAST_ERROR.set(&e);
            -1
        }
    }
}

/// Open the first available XR50 device.
/// Returns NULL on error (check xv_last_error()).
#[no_mangle]
pub extern "C" fn xv_open_first() -> *mut XvDevice {
    match Device::open_first() {
        Ok(dev) => Box::into_raw(Box::new(XvDevice(dev))),
        Err(e) => {
            LAST_ERROR.set(&e);
            std::ptr::null_mut()
        }
    }
}

/// Open a specific XR50 device by its info.
/// Returns NULL on error.
///
/// # Safety
/// `info` must point to a valid `XvDeviceInfo`, or be null.
#[no_mangle]
pub unsafe extern "C" fn xv_open_device(info: *const XvDeviceInfo) -> *mut XvDevice {
    if info.is_null() {
        return std::ptr::null_mut();
    }

    let info = &*info;

    let uuid = c_char_to_string(&info.uuid);
    let version = c_char_to_string(&info.version);
    let bus_id = c_char_to_string(&info.bus_id);

    let dev_info = crate::types::DeviceInfo {
        uuid,
        version,
        features: crate::types::Features::from_bits_truncate(info.features),
        bus_id,
        device_address: info.address,
    };

    match Device::open(&dev_info) {
        Ok(dev) => Box::into_raw(Box::new(XvDevice(dev))),
        Err(e) => {
            LAST_ERROR.set(&e);
            std::ptr::null_mut()
        }
    }
}

/// Close a device and free its resources.
///
/// # Safety
/// `dev` must be a pointer returned by `xv_open_first` or `xv_open_device`, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_close_device(dev: *mut XvDevice) {
    if !dev.is_null() {
        drop(Box::from_raw(dev));
    }
}

/// Get the device UUID. Returns a pointer to a null-terminated string
/// valid for the lifetime of the device.
///
/// # Safety
/// `dev` must be a valid device pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_device_uuid(dev: *const XvDevice) -> *const c_char {
    if dev.is_null() {
        return std::ptr::null();
    }
    let dev = &*dev;
    dev.0.uuid().as_ptr() as *const c_char
}

/// Get the device firmware version string.
///
/// # Safety
/// `dev` must be a valid device pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_device_version(dev: *const XvDevice) -> *const c_char {
    if dev.is_null() {
        return std::ptr::null();
    }
    let dev = &*dev;
    dev.0.version().as_ptr() as *const c_char
}

/// Get the device feature bitmap.
///
/// # Safety
/// `dev` must be a valid device pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_device_features(dev: *const XvDevice) -> u32 {
    if dev.is_null() {
        return 0;
    }
    let dev = &*dev;
    dev.0.features().bits()
}

/// Start SLAM streaming.
/// `mode`: 0 = Edge, 1 = Mixed.
/// Returns NULL on error.
///
/// # Safety
/// `dev` must be a valid device pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_start_slam(dev: *mut XvDevice, mode: c_int) -> *mut XvSlamStream {
    if dev.is_null() {
        return std::ptr::null_mut();
    }
    let dev = &mut *dev;
    let slam_mode = match mode {
        0 => SlamMode::Edge,
        1 => SlamMode::Mixed,
        _ => SlamMode::Edge,
    };

    match dev.0.start_slam(slam_mode) {
        Ok(stream) => Box::into_raw(Box::new(XvSlamStream(stream))),
        Err(e) => {
            LAST_ERROR.set(&e);
            std::ptr::null_mut()
        }
    }
}

/// Receive the next SLAM pose with timeout.
/// `timeout_ms`: timeout in milliseconds (0 = try without blocking, -1 = block forever).
/// Returns 0 on success, -1 on error/timeout.
///
/// # Safety
/// `stream` and `pose` must be valid pointers, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_slam_recv(
    stream: *mut XvSlamStream,
    pose: *mut XvPose,
    timeout_ms: c_int,
) -> c_int {
    if stream.is_null() || pose.is_null() {
        return -1;
    }
    let stream = &*stream;

    let result = if timeout_ms == 0 {
        stream.0.try_recv().ok_or(crate::XvisioError::Timeout)
    } else if timeout_ms < 0 {
        stream.0.recv()
    } else {
        stream
            .0
            .recv_timeout(Duration::from_millis(timeout_ms as u64))
    };

    match result {
        Ok(sample) => {
            let out = XvPose {
                translation: sample.pose.translation,
                rotation: [
                    sample.pose.rotation[0][0],
                    sample.pose.rotation[0][1],
                    sample.pose.rotation[0][2],
                    sample.pose.rotation[1][0],
                    sample.pose.rotation[1][1],
                    sample.pose.rotation[1][2],
                    sample.pose.rotation[2][0],
                    sample.pose.rotation[2][1],
                    sample.pose.rotation[2][2],
                ],
                quaternion: sample.pose.quaternion,
                timestamp_us: sample.pose.timestamp_us,
                host_timestamp_s: sample.pose.host_timestamp_s,
                confidence: sample.pose.confidence,
                euler_deg: sample.pose.euler_deg,
            };
            pose.write(out);
            0
        }
        Err(e) => {
            LAST_ERROR.set(&e);
            -1
        }
    }
}

/// Check if the SLAM stream is still active.
///
/// # Safety
/// `stream` must be a valid stream pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_slam_is_active(stream: *const XvSlamStream) -> bool {
    if stream.is_null() {
        return false;
    }
    let stream = &*stream;
    stream.0.is_active()
}

/// Stop a SLAM stream and free its resources.
///
/// # Safety
/// `stream` must be a pointer returned by `xv_start_slam`, or null.
#[no_mangle]
pub unsafe extern "C" fn xv_stop_slam(stream: *mut XvSlamStream) {
    if !stream.is_null() {
        drop(Box::from_raw(stream));
    }
}

/// Get the last error message. Returns NULL if no error.
/// The returned pointer is valid until the next xvisio API call.
#[no_mangle]
pub extern "C" fn xv_last_error() -> *const c_char {
    LAST_ERROR.as_ptr()
}

fn c_char_to_string(buf: &[c_char]) -> String {
    let end = buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..end].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).to_string()
}
