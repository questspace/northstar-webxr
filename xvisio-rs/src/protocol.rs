use crate::types::{Features, ImuData, Pose, SlamSample};
use std::sync::OnceLock;
use std::time::Instant;

// -- USB identifiers --
pub const VID: u16 = 0x040E;
pub const PID: u16 = 0xF408;
pub const HID_INTERFACE: u8 = 3;
pub const SLAM_ENDPOINT: u8 = 0x83;

// -- Packet geometry --
pub const REPORT_SIZE: usize = 63;

/// Fixed-point scale factor: 2^(-14) = 1/16384.
pub const SCALE: f64 = 6.103515625e-05;

// -- Command direction prefixes --
pub const PREFIX_HOST_TO_DEVICE: u8 = 0x02;
pub const PREFIX_DEVICE_TO_HOST: u8 = 0x01;

// -- Command bytes (after the 0x02 prefix) --
pub const CMD_UUID: &[u8] = &[0xFD, 0x66, 0x00, 0x02];
pub const CMD_VERSION: &[u8] = &[0x1C, 0x99];
pub const CMD_FEATURES: &[u8] = &[0xDE, 0x62, 0x01];
pub const CMD_CONFIGURE: &[u8] = &[0x19, 0x95];
pub const CMD_EDGE_STREAM: &[u8] = &[0xA2, 0x33];
pub const CMD_STEREO_CAMERA_INIT: &[u8] = &[0xFE, 0x20, 0x21];
pub const CMD_STEREO_CAMERA_START: &[u8] = &[0xFE, 0x20, 0x22];
/// IMU stream on/off (`[0xBD, 0xDF, 0x01 | 0x00]`), found in the official SDK's
/// `xslam-hid-sdk.dll` next to the edge-stream command; starts `01 BD DF` reports at ~1 kHz.
pub const CMD_IMU_STREAM: &[u8] = &[0xBD, 0xDF];

// -- SLAM packet header echo --
pub const SLAM_HEADER: [u8; 3] = [0x01, 0xA2, 0x33];
pub const IMU_HEADER: [u8; 3] = [0x01, 0xBD, 0xDF];

/// Standard gravity, for the accelerometer's g → m/s².
const G: f64 = 9.80665;

/// Start or stop the IMU stream.
pub fn build_imu_stream_cmd(start: bool) -> [u8; REPORT_SIZE] {
    build_command(&[CMD_IMU_STREAM[0], CMD_IMU_STREAM[1], start as u8])
}

/// Parse an IMU report (verified against the official SDK on 2026-10-26):
/// - `[0..3]`: `01 BD DF`, `[3..7]`: device timestamp (µs, u32 LE)
/// - `[15..27]`: accelerometer x, y, z (f32 LE, g)
/// - `[27..39]`: gyroscope x, y, z (f32 LE, rad/s)
/// - `[39..51]`: magnetometer x, y, z (f32 LE, likely µT)
pub fn parse_imu_packet(data: &[u8]) -> Option<ImuData> {
    if data.len() < 51 || data[..3] != IMU_HEADER {
        return None;
    }
    let f = |o: usize| f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as f64;
    let triad = |o: usize, scale: f64| [f(o) * scale, f(o + 4) * scale, f(o + 8) * scale];
    Some(ImuData {
        timestamp_us: u32::from_le_bytes([data[3], data[4], data[5], data[6]]),
        accelerometer: triad(15, G),
        gyroscope: triad(27, 1.0),
        magnetometer: triad(39, 1.0),
    })
}

/// Build a 63-byte HID command buffer.
/// Format: [0x02, cmd_bytes..., 0x00 padding...]
pub fn build_command(cmd: &[u8]) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = PREFIX_HOST_TO_DEVICE;
    let len = cmd.len().min(REPORT_SIZE - 1);
    buf[1..1 + len].copy_from_slice(&cmd[..len]);
    buf
}

/// Build the configure command for a given SLAM mode.
/// Edge mode: [0x19, 0x95, 0x01, 0x01, 0x00]
/// Mixed mode: [0x19, 0x95, 0x01, 0x01, 0x01]
pub fn build_configure_cmd_with_uvc(
    edge: bool,
    uvc_mode: u8,
    embedded_algo: bool,
) -> [u8; REPORT_SIZE] {
    let mut cmd_bytes = [0u8; 5];
    cmd_bytes[0..2].copy_from_slice(CMD_CONFIGURE);
    cmd_bytes[2] = if edge { 1 } else { 0 };
    cmd_bytes[3] = uvc_mode;
    cmd_bytes[4] = if embedded_algo { 1 } else { 0 };
    build_command(&cmd_bytes)
}

/// Build the configure command with the default UVC mode.
///
/// Default is `uvcMode=0` to preserve existing Windows/Linux behavior.
pub fn build_configure_cmd(edge: bool, embedded_algo: bool) -> [u8; REPORT_SIZE] {
    build_configure_cmd_with_uvc(edge, 0, embedded_algo)
}

/// Build the start/stop edge stream command.
/// Start: [0xA2, 0x33, 0x01, 0x01, 0x00] (rotationEnabled=true per C++ libxvisio)
/// Stop:  [0xA2, 0x33, 0x00, 0x00, 0x00]
pub fn build_edge_stream_cmd_with_params(
    edge_mode: u8,
    rotation_enabled: bool,
    flipped: bool,
) -> [u8; REPORT_SIZE] {
    let mut cmd_bytes = [0u8; 5];
    cmd_bytes[0..2].copy_from_slice(CMD_EDGE_STREAM);
    cmd_bytes[2] = edge_mode;
    cmd_bytes[3] = if rotation_enabled { 1 } else { 0 };
    cmd_bytes[4] = if flipped { 1 } else { 0 };
    build_command(&cmd_bytes)
}

/// Build the start/stop edge stream command with default parameters.
pub fn build_edge_stream_cmd(start: bool) -> [u8; REPORT_SIZE] {
    build_edge_stream_cmd_with_params(if start { 1 } else { 0 }, start, false)
}

/// Build the stereo camera init command for macOS.
/// On macOS, after USB re-enumeration from detach_kernel_driver, the cameras
/// are left uninitiated. Without this command, SLAM outputs identity poses.
pub fn build_stereo_camera_init_cmd() -> [u8; REPORT_SIZE] {
    build_command(CMD_STEREO_CAMERA_INIT)
}

/// Build the stereo camera start command for macOS.
/// Sent after stereo_camera_init to start the camera feed for SLAM processing.
pub fn build_stereo_camera_start_cmd() -> [u8; REPORT_SIZE] {
    build_command(CMD_STEREO_CAMERA_START)
}

/// Extract the command echo from a response and return the payload start offset.
/// Response format: [0x01, cmd_echo..., payload...]
pub fn validate_response(response: &[u8], expected_cmd: &[u8]) -> crate::Result<usize> {
    if response.is_empty() || response[0] != PREFIX_DEVICE_TO_HOST {
        return Err(crate::XvisioError::InvalidResponse(
            response.first().copied().unwrap_or(0),
        ));
    }
    let cmd_len = expected_cmd.len();
    if response.len() < 1 + cmd_len {
        return Err(crate::XvisioError::CommandMismatch);
    }
    if &response[1..1 + cmd_len] != expected_cmd {
        return Err(crate::XvisioError::CommandMismatch);
    }
    Ok(1 + cmd_len)
}

/// Extract a null-terminated string from a byte slice.
pub fn extract_string(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Parse features bitmap from response payload (little-endian u32).
pub fn parse_features(payload: &[u8]) -> Features {
    if payload.len() < 4 {
        return Features::empty();
    }
    let bits = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    Features::from_bits_truncate(bits)
}

/// Convert XR50 quaternion [w, x, y, z] to Euler angles [roll, pitch, yaw] in degrees
/// for the Three.js frontend (YXZ order with Z-axis flip).
///
/// The XR50 uses Z-forward, Three.js uses Z-backward. We apply the Z-flip to the
/// rotation matrix (R' = T·R·T where T = diag(1,1,-1)), then extract YXZ Euler
/// angles to match the frontend's `new THREE.Euler(pitch, yaw, roll, 'YXZ')`.
///
/// From the Z-flipped rotation matrix R':
///   roll  (Euler.z) = atan2(R'[1][0], R'[1][1]) = atan2(2(xy+wz), 1 - 2(x²+z²))
///   pitch (Euler.x) = asin(-R'[1][2])            = asin(2(yz-wx))
///   yaw   (Euler.y) = atan2(R'[0][2], R'[2][2])  = atan2(-2(xz+wy), 1 - 2(x²+y²))
pub fn quaternion_to_euler(w: f64, x: f64, y: f64, z: f64) -> [f64; 3] {
    let roll = (2.0 * (x * y + w * z)).atan2(1.0 - 2.0 * (x * x + z * z));
    let pitch = (2.0 * (y * z - w * x)).clamp(-1.0, 1.0).asin();
    let yaw = (-2.0 * (x * z + w * y)).atan2(1.0 - 2.0 * (x * x + y * y));
    [roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees()]
}

/// Convert quaternion [w, x, y, z] to a 3x3 rotation matrix (row-major).
fn quaternion_to_rotation(w: f64, x: f64, y: f64, z: f64) -> [[f64; 3]; 3] {
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// Convert a 3x3 rotation matrix to quaternion [w, x, y, z].
fn rotation_to_quaternion(m: &[[f64; 3]; 3]) -> [f64; 4] {
    let trace = m[0][0] + m[1][1] + m[2][2];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
        ]
    }
}

fn parse_rotation_matrix(data: &[u8]) -> [[f64; 3]; 3] {
    let mut rot = [[0.0f64; 3]; 3];
    let mut idx = 19usize;
    for row in &mut rot {
        for cell in row {
            *cell = i16::from_le_bytes([data[idx], data[idx + 1]]) as f64 * SCALE;
            idx += 2;
        }
    }
    rot
}

#[derive(Clone, Copy)]
enum RotationParseMode {
    Auto,
    Matrix,
    Quaternion,
}

fn rotation_parse_mode() -> RotationParseMode {
    static MODE: OnceLock<RotationParseMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        match std::env::var("XVISIO_ROTATION_PARSE")
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("matrix") => RotationParseMode::Matrix,
            Some("quat") | Some("quaternion") => RotationParseMode::Quaternion,
            _ => RotationParseMode::Auto,
        }
    })
}

/// Validate a quaternion parsed from packet bytes.
/// XR50 wire quaternion should be close to unit length.
fn is_plausible_quaternion(w: f64, x: f64, y: f64, z: f64) -> bool {
    let norm = (w * w + x * x + y * y + z * z).sqrt();
    w.abs() <= 1.5
        && x.abs() <= 1.5
        && y.abs() <= 1.5
        && z.abs() <= 1.5
        && (0.5..=1.5).contains(&norm)
}

/// Some XR50 firmware stores rotation as a 3x3 matrix at bytes [19..36]; others
/// (e.g. xr50 V1.09) store a quaternion at [19..26]. The same bytes decode as
/// either, so auto-detection must be strict: a real rotation matrix has
/// orthonormal rows AND determinant +1. Requiring the determinant is essential —
/// without it, a quaternion firmware's bytes intermittently pass as a "matrix"
/// (det ≈ 0.6), causing the parser to flip between two ~170°-apart orientations.
fn is_plausible_rotation_matrix(m: &[[f64; 3]; 3]) -> bool {
    let norm = |r: usize| m[r][0] * m[r][0] + m[r][1] * m[r][1] + m[r][2] * m[r][2];
    let dot = |a: usize, b: usize| m[a][0] * m[b][0] + m[a][1] * m[b][1] + m[a][2] * m[b][2];
    if !(0.8..=1.2).contains(&norm(0))
        || !(0.8..=1.2).contains(&norm(1))
        || !(0.8..=1.2).contains(&norm(2))
    {
        return false;
    }
    if dot(0, 1).abs() >= 0.3 || dot(0, 2).abs() >= 0.3 || dot(1, 2).abs() >= 0.3 {
        return false;
    }
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    (0.85..=1.15).contains(&det)
}

/// Convert a 3x3 rotation matrix to Euler angles [roll, pitch, yaw] in degrees.
/// Kept for backwards compatibility. Prefer quaternion_to_euler for SLAM data.
pub fn rotation_to_euler(m: &[[f64; 3]; 3]) -> [f64; 3] {
    let pitch = (-m[2][0]).asin();
    let (roll, yaw) = if pitch.cos().abs() > 1e-6 {
        let roll = m[2][1].atan2(m[2][2]);
        let yaw = m[1][0].atan2(m[0][0]);
        (roll, yaw)
    } else {
        let roll = m[0][1].atan2(m[1][1]);
        (roll, 0.0)
    };
    [roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees()]
}

/// Parse a 63-byte SLAM packet into a SlamSample.
///
/// Packet layout:
/// - `[0]`: 0x01 (response indicator)
/// - `[1..2]`: 0xA2, 0x33 (command echo)
/// - `[3..6]`: uint32 LE timestamp (microseconds)
/// - `[7..18]`: 3x int32 LE translation (scaled by 2^-14)
/// - `[19..36]`: 9x int16 LE 3x3 rotation matrix (scaled by 2^-14)
/// - `[19..26]`: optional quaternion fallback for non-standard packets
/// - `[37..62]`: extended data (IMU, confidence, padding)
pub fn parse_slam_packet(data: &[u8], epoch: Instant) -> Option<SlamSample> {
    if data.len() < REPORT_SIZE {
        return None;
    }

    // Validate header
    if data[0] != SLAM_HEADER[0] || data[1] != SLAM_HEADER[1] || data[2] != SLAM_HEADER[2] {
        return None;
    }

    let host_timestamp_s = epoch.elapsed().as_secs_f64();

    // Timestamp (uint32 LE)
    let timestamp_us = u32::from_le_bytes([data[3], data[4], data[5], data[6]]) as u64;

    // Translation (3x int32 LE, scaled)
    let tx = i32::from_le_bytes([data[7], data[8], data[9], data[10]]) as f64 * SCALE;
    let ty = i32::from_le_bytes([data[11], data[12], data[13], data[14]]) as f64 * SCALE;
    let tz = i32::from_le_bytes([data[15], data[16], data[17], data[18]]) as f64 * SCALE;

    let parse_quaternion = || {
        let w = i16::from_le_bytes([data[19], data[20]]) as f64 * SCALE;
        let x = i16::from_le_bytes([data[21], data[22]]) as f64 * SCALE;
        let y = i16::from_le_bytes([data[23], data[24]]) as f64 * SCALE;
        let z = i16::from_le_bytes([data[25], data[26]]) as f64 * SCALE;
        (quaternion_to_rotation(w, x, y, z), w, x, y, z)
    };

    let (rotation, qw, qx, qy, qz) = match rotation_parse_mode() {
        RotationParseMode::Quaternion => parse_quaternion(),
        RotationParseMode::Matrix => {
            let m = parse_rotation_matrix(data);
            let [w, x, y, z] = rotation_to_quaternion(&m);
            (m, w, x, y, z)
        }
        RotationParseMode::Auto => {
            let quaternion_candidate = parse_quaternion();
            let matrix_candidate = parse_rotation_matrix(data);
            if is_plausible_rotation_matrix(&matrix_candidate) {
                let [w, x, y, z] = rotation_to_quaternion(&matrix_candidate);
                (matrix_candidate, w, x, y, z)
            } else if is_plausible_quaternion(
                quaternion_candidate.1,
                quaternion_candidate.2,
                quaternion_candidate.3,
                quaternion_candidate.4,
            ) {
                quaternion_candidate
            } else {
                quaternion_candidate
            }
        }
    };

    // Store as [qx, qy, qz, qw] (SDK-facing convention).
    let quaternion = [qx, qy, qz, qw];
    let euler_deg = quaternion_to_euler(qw, qx, qy, qz);

    // Extended data [37..62]
    let mut raw_extended = [0u8; 26];
    raw_extended.copy_from_slice(&data[37..63]);

    // The IMU comes from its own report stream (`parse_imu_packet`); the reader attaches
    // the latest sample. (Bytes 39/41/43 here hold a coarse 1/128 g accelerometer copy.)
    let imu = None;

    // Confidence from bytes [57..58] scaled
    let confidence_raw = i16::from_le_bytes([data[57], data[58]]) as f64 * SCALE;
    let confidence = confidence_raw.clamp(0.0, 1.0);

    Some(SlamSample {
        pose: Pose {
            translation: [tx, ty, tz],
            rotation,
            quaternion,
            timestamp_us,
            host_timestamp_s,
            confidence,
            euler_deg,
        },
        imu,
        raw_extended,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_command() {
        let buf = build_command(CMD_UUID);
        assert_eq!(buf[0], 0x02);
        assert_eq!(&buf[1..5], CMD_UUID);
        assert_eq!(buf[5], 0);
    }

    #[test]
    fn test_validate_response() {
        let mut resp = [0u8; 63];
        resp[0] = 0x01;
        resp[1..5].copy_from_slice(CMD_UUID);
        resp[5] = b'X';
        let offset = validate_response(&resp, CMD_UUID).unwrap();
        assert_eq!(offset, 5);
    }

    #[test]
    fn test_quaternion_to_euler_identity() {
        let euler = quaternion_to_euler(1.0, 0.0, 0.0, 0.0);
        assert!(euler[0].abs() < 1e-10); // roll
        assert!(euler[1].abs() < 1e-10); // pitch
        assert!(euler[2].abs() < 1e-10); // yaw
    }

    #[test]
    fn test_quaternion_to_rotation_identity() {
        let m = quaternion_to_rotation(1.0, 0.0, 0.0, 0.0);
        assert!((m[0][0] - 1.0).abs() < 1e-10);
        assert!((m[1][1] - 1.0).abs() < 1e-10);
        assert!((m[2][2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_parse_slam_packet() {
        // Example packet from PROTOCOL.md
        let data: [u8; 63] = [
            0x01, 0xa2, 0x33, 0x6b, 0xd1, 0x25, 0x5f, 0x58, 0x01, 0x00, 0x00, 0x1e, 0x00, 0x00,
            0x00, 0xc3, 0x01, 0x00, 0x00, 0x62, 0xc0, 0x3a, 0x03, 0x2d, 0x06, 0x5a, 0xfd, 0x56,
            0xc0, 0xf3, 0x05, 0x72, 0x06, 0xa9, 0x05, 0x6c, 0x3f, 0xa0, 0x56, 0x7d, 0x00, 0xf3,
            0xff, 0xf2, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x09, 0x00, 0x07,
            0x00, 0x2b, 0x41, 0x00, 0x00, 0x00, 0x00,
        ];

        let epoch = Instant::now();
        let sample = parse_slam_packet(&data, epoch).unwrap();

        // Timestamp = 1596313963 µs
        assert_eq!(sample.pose.timestamp_us, 1596313963);

        // Translation should be approximately [0.0210, 0.0018, 0.0275]
        assert!((sample.pose.translation[0] - 0.0210).abs() < 0.001);
        assert!((sample.pose.translation[1] - 0.0018).abs() < 0.001);
        assert!((sample.pose.translation[2] - 0.0275).abs() < 0.001);

        // libxvisio stores bytes [19..36] as a 3x3 rotation matrix.
        assert!((sample.pose.rotation[0][0] - (-0.994)).abs() < 0.01);
        assert!((sample.pose.rotation[1][1] - (-0.995)).abs() < 0.01);
        assert!((sample.pose.rotation[2][2] - 0.991).abs() < 0.01);

        // The SDK-facing quaternion is derived from that matrix.
        assert!((sample.pose.quaternion[2] - 0.997).abs() < 0.01);
        let qn = (sample.pose.quaternion[0] * sample.pose.quaternion[0]
            + sample.pose.quaternion[1] * sample.pose.quaternion[1]
            + sample.pose.quaternion[2] * sample.pose.quaternion[2]
            + sample.pose.quaternion[3] * sample.pose.quaternion[3])
            .sqrt();
        assert!((qn - 1.0).abs() < 0.05);
    }

    /// A real `01 BD DF` report captured next to the official SDK, which printed
    /// Accel(-0.29, 9.68, -1.31) m/s² for the same still device.
    #[test]
    fn test_parse_imu_packet() {
        let hex = "01bddfc2f4f30000000000000000009311783f5c3212bd69732fbe277251bb835af43b6fa18b3b145b00c2ec1781423833014233d3a0430000000001000000";
        let data: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        let imu = parse_imu_packet(&data).expect("IMU report");
        assert_eq!(imu.timestamp_us, 15987906);
        let g: f64 = imu.accelerometer.iter().map(|a| a * a).sum::<f64>().sqrt();
        assert!((g - 9.66).abs() < 0.1, "|accel| = {g}");
        assert!(imu.gyroscope.iter().all(|w| w.abs() < 0.02), "still device");
        assert!(parse_imu_packet(&data[..40]).is_none());
        assert!(parse_slam_packet(&data, std::time::Instant::now()).is_none());
    }
}
