/// 6DOF pose from the XR50 edge SLAM.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    /// Translation in meters [x, y, z].
    pub translation: [f64; 3],
    /// 3x3 row-major rotation matrix.
    pub rotation: [[f64; 3]; 3],
    /// Quaternion [qx, qy, qz, qw] matching official XVisio SDK convention.
    pub quaternion: [f64; 4],
    /// Edge timestamp in microseconds.
    pub timestamp_us: u64,
    /// Host steady-clock timestamp in seconds.
    pub host_timestamp_s: f64,
    /// Tracking confidence [0..1]. Derived from extended packet data.
    pub confidence: f64,
    /// Euler angles [roll, pitch, yaw] in degrees (YXZ order with Z-flip for Three.js).
    /// roll = head tilt (Euler.z), pitch = look up/down (Euler.x), yaw = turn left/right (Euler.y).
    pub euler_deg: [f64; 3],
}

/// Raw IMU data parsed from extended SLAM packet bytes [37..48].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ImuData {
    /// Device clock in microseconds, the same clock as `Pose::timestamp_us`.
    pub timestamp_us: u32,
    /// Accelerometer [x, y, z] in the IMU frame, m/s².
    pub accelerometer: [f64; 3],
    /// Gyroscope [x, y, z] in the IMU frame, rad/s.
    pub gyroscope: [f64; 3],
    /// Third triad of the IMU report: slowly varying, magnitude ~78 at the desk, most
    /// likely a magnetometer in µT (unverified).
    pub magnetometer: [f64; 3],
}

/// Full SLAM sample including pose, optional IMU, and raw extended data.
#[derive(Debug, Clone)]
pub struct SlamSample {
    pub pose: Pose,
    /// Latest IMU sample when the pose arrived: None in edge SLAM, which runs without the
    /// IMU stream (see `Device::start_imu`).
    pub imu: Option<ImuData>,
    /// Raw bytes [37..62] from the SLAM packet for user analysis.
    pub raw_extended: [u8; 26],
}

/// Device identification and capabilities.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub uuid: String,
    pub version: String,
    pub features: Features,
    pub bus_id: String,
    pub device_address: u8,
}

bitflags::bitflags! {
    /// Feature bitmap reported by the XR50 device.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(C)]
    pub struct Features: u32 {
        const EDGE_MODE    = 1 << 0;
        const MIXED_MODE   = 1 << 1;
        const STEREO       = 1 << 2;
        const RGB          = 1 << 3;
        const TOF          = 1 << 4;
        const IA           = 1 << 5;
        const SGBM         = 1 << 6;
        const EYE_TRACKING = 1 << 10;
        const FACE_ID      = 1 << 12;
    }
}

/// SLAM operating mode.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlamMode {
    /// On-device SLAM processing (edge6dof=1, embeddedAlgo=0).
    Edge = 0,
    /// Mixed host+device SLAM processing (edge6dof=0, embeddedAlgo=1).
    Mixed = 1,
}
