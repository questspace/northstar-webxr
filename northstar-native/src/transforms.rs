//! Same frame conventions as visual-test/src/lib/transforms.ts (see PROTOCOL.md).
use crate::calibration::Calibration;
use glam::{Mat3, Mat4, Quat, Vec3};
use std::f32::consts::PI;

/// Height of the SLAM origin above the virtual floor.
pub const EYE_HEIGHT: f32 = 1.6;

/// XR50 world frame (X left, Y up, Z forward) → head frame: 180° about Y.
fn world_to_three() -> Quat { Quat::from_rotation_y(PI) }
/// Device axes are X right, Y down, Z forward (camera convention) → camera: 180° about X.
fn device_to_camera() -> Quat { Quat::from_rotation_x(PI) }

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Default for Pose {
    fn default() -> Self { Self { position: Vec3::ZERO, rotation: Quat::IDENTITY } }
}

/// Raw XR50 pose ([x,y,z] m, [qx,qy,qz,qw]) → tracker pose in the head frame.
pub fn xr50_to_three(translation: [f64; 3], quaternion: [f64; 4]) -> Pose {
    let q = Quat::from_xyzw(quaternion[0] as f32, quaternion[1] as f32, quaternion[2] as f32, quaternion[3] as f32);
    Pose {
        position: Vec3::new(-translation[0] as f32, translation[1] as f32, -translation[2] as f32),
        rotation: world_to_three() * q * device_to_camera(),
    }
}

/// Tracker pose → camera rig (tracker→head calibration is identity in EskySettings.json).
pub fn rig(pose: &Pose) -> Mat4 {
    Mat4::from_rotation_translation(pose.rotation, pose.position + Vec3::Y * EYE_HEIGHT)
}

/// Leap sensor frame → head frame for the HMD mount (measured: hand right → Leap -X,
/// up → Leap -Z, forward → Leap +Y), then the calibrated sensor tilt and offset.
pub struct LeapToHead { rotation: Quat, offset: Vec3 }

impl LeapToHead {
    pub fn new(c: &Calibration) -> Self {
        let mount = Quat::from_mat3(&Mat3::from_cols(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, -1.0, 0.0),
        ));
        Self { rotation: c.leap_rotation * mount, offset: c.leap_translation }
    }

    pub fn point(&self, mm: Vec3) -> Vec3 { self.rotation * (mm * 0.001) + self.offset }
    /// A rotation expressed in the Leap frame → the same rotation in the head frame.
    pub fn rotate(&self, q: Quat) -> Quat { self.rotation * q * self.rotation.inverse() }
}

/// Camera-space ray rotation taking the displayed pose back to the rendered one.
pub fn reprojection(render: Quat, display: Quat) -> Mat3 {
    Mat3::from_quat(render.inverse() * display)
}
