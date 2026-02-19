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
//! let device = Device::open_first().unwrap();
//! println!("UUID: {}", device.uuid());
//!
//! let stream = device.start_slam(SlamMode::Edge).unwrap();
//! for _ in 0..100 {
//!     let sample = stream.recv_timeout(Duration::from_secs(1)).unwrap();
//!     println!("pos: {:?}", sample.pose.translation);
//! }
//! ```

pub mod error;
pub mod types;
pub mod protocol;
pub mod hid;
pub mod device;
pub mod slam;
pub mod ffi;

pub use error::XvisioError;
pub use types::*;
pub use device::Device;
pub use slam::SlamStream;

/// Result type alias for xvisio operations.
pub type Result<T> = std::result::Result<T, XvisioError>;
