//! # xvisio - Rust SDK for XVisio XR50 6DOF Tracking Sensor
//!
//! Cross-platform driver using hidapi. Provides:
//! - Device discovery and info queries (UUID, firmware version, features)
//! - High-performance SLAM streaming at ~950 Hz
//! - C FFI for integration with C/C++/Unity/Swift
//!
//! ## Quick Start
//! ```no_run
//! use xvisio::{Device, SlamMode};
//! use std::time::Duration;
//!
//! let mut device = Device::open_first().unwrap();
//! println!("UUID: {}", device.uuid());
//!
//! let stream = device.start_slam(SlamMode::Edge).unwrap();
//! for _ in 0..100 {
//!     let sample = stream.recv_timeout(Duration::from_secs(1)).unwrap();
//!     println!("pos: {:?}", sample.pose.translation);
//! }
//! ```

pub mod device;
pub mod error;
pub mod ffi;
pub mod hid;
pub mod protocol;
pub mod slam;
pub mod types;

pub use device::Device;
pub use error::XvisioError;
pub use slam::SlamStream;
pub use types::*;

/// Result type alias for xvisio operations.
pub type Result<T> = std::result::Result<T, XvisioError>;
