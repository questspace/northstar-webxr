use std::fmt;

/// Errors that can occur when interacting with the XR50 device.
#[derive(Debug, thiserror::Error)]
pub enum XvisioError {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("Device not found (VID=040E PID=F408)")]
    DeviceNotFound,

    #[error("HID command failed: {0}")]
    HidCommand(String),

    #[error("Invalid response: expected prefix 0x01, got 0x{0:02x}")]
    InvalidResponse(u8),

    #[error("Command echo mismatch")]
    CommandMismatch,

    #[error("SLAM stream stopped")]
    StreamStopped,

    #[error("Timeout waiting for data")]
    Timeout,

    #[error("Channel disconnected")]
    ChannelDisconnected,
}

/// Thread-safe last-error storage for the C FFI layer.
pub(crate) struct LastError {
    message: std::sync::Mutex<String>,
}

impl LastError {
    pub const fn new() -> Self {
        Self {
            message: std::sync::Mutex::new(String::new()),
        }
    }

    pub fn set(&self, err: &XvisioError) {
        if let Ok(mut msg) = self.message.lock() {
            *msg = fmt::format(format_args!("{}\0", err));
        }
    }

    pub fn as_ptr(&self) -> *const std::ffi::c_char {
        match self.message.lock() {
            Ok(msg) if !msg.is_empty() => msg.as_ptr() as *const std::ffi::c_char,
            _ => std::ptr::null(),
        }
    }
}
