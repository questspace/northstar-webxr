//! Same frame conventions as visual-test/src/lib/transforms.ts (see PROTOCOL.md).
use crate::calibration::Calibration;
use glam::{Mat3, Mat4, Quat, Vec3};
use std::f32::consts::PI;

/// Height of the SLAM origin above the virtual floor.
pub const EYE_HEIGHT: f32 = 1.6;

/// XR50 world frame is camera convention (X right, Y down, Z forward) → head frame: 180° about X.
fn world_to_three() -> Quat { Quat::from_rotation_x(PI) }
/// Device axes are camera convention too → GL camera (X right, Y up, Z back): 180° about X.
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
        position: Vec3::new(translation[0] as f32, -translation[1] as f32, -translation[2] as f32),
        rotation: world_to_three() * q * device_to_camera(),
    }
}

/// Tracker pose → camera rig (tracker→head calibration is identity in EskySettings.json).
pub fn rig(pose: &Pose) -> Mat4 {
    Mat4::from_rotation_translation(pose.rotation, pose.position + Vec3::Y * EYE_HEIGHT)
}

/// Leap sensor frame → head frame for the HMD mount (measured: hand right → Leap -X,
/// up → Leap -Z, forward → Leap +Y), then the calibrated sensor tilt and offset, then rolled 180°
/// about the view axis into the rig: the rig is the real head frame rolled 180° (the Esky warp
/// expects the panel rotated 180°, and the XR50 conversion was tuned against that). Found on the
/// head 2026-10-27: hands moved opposite on both screen axes with depth correct.
pub struct LeapToHead { rotation: Quat, offset: Vec3 }

impl LeapToHead {
    pub fn new(c: &Calibration) -> Self {
        let mount = Quat::from_mat3(&Mat3::from_cols(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, -1.0, 0.0),
        ));
        let head_to_rig = Quat::from_rotation_z(std::f32::consts::PI);
        Self { rotation: head_to_rig * c.leap_rotation * mount, offset: head_to_rig * c.leap_translation }
    }

    pub fn point(&self, mm: Vec3) -> Vec3 { self.rotation * (mm * 0.001) + self.offset }
    /// A rotation expressed in the Leap frame → the same rotation in the head frame.
    pub fn rotate(&self, q: Quat) -> Quat { self.rotation * q * self.rotation.inverse() }
}

/// Camera-space ray rotation taking the displayed pose back to the rendered one.
pub fn reprojection(render: Quat, display: Quat) -> Mat3 {
    Mat3::from_quat(render.inverse() * display)
}
