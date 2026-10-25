//! EskySettings.json → the subset the native pipeline needs, converted to the
//! Three.js-style head frame (X right, Y up, Z back) used across vibestar.
use glam::{Quat, Vec3};
use serde::Deserialize;

#[derive(Deserialize, Clone, Copy)]
struct V3 { x: f32, y: f32, z: f32 }
#[derive(Deserialize, Clone, Copy)]
struct V4 { x: f32, y: f32, z: f32, w: f32 }

#[derive(Deserialize)]
struct Offsets {
    #[serde(rename = "TranslationEyeToLeapMotion")] leap_translation: V3,
    #[serde(rename = "RotationEyeToLeapMotion")] leap_rotation: V4,
}

#[derive(Deserialize)]
struct V2 {
    left_uv_to_rect_x: [f32; 16],
    left_uv_to_rect_y: [f32; 16],
    right_uv_to_rect_x: [f32; 16],
    right_uv_to_rect_y: [f32; 16],
}

#[derive(Deserialize)]
struct Eye { #[serde(rename = "eyePosition")] eye_position: V3 }

#[derive(Deserialize)]
struct V1 { #[serde(rename = "leftEye")] left: Eye, #[serde(rename = "rightEye")] right: Eye }

#[derive(Deserialize)]
struct DisplayJson {
    #[serde(rename = "DisplayXLoc")] x: i32,
    #[serde(rename = "DisplayYLoc")] y: i32,
    #[serde(rename = "DisplayWidth")] width: u32,
    #[serde(rename = "DisplayHeight")] height: u32,
}

#[derive(Deserialize)]
struct Esky {
    #[serde(rename = "myOffsets")] offsets: Offsets,
    #[serde(rename = "v2CalibrationValues")] v2: V2,
    #[serde(rename = "v1CalibrationValues")] v1: V1,
    #[serde(rename = "displayWindowSettings")] display: DisplayJson,
}

/// Unity is left-handed with Z forward: mirror Z for points, negate x/y for rotations.
fn unity_vec(v: V3) -> Vec3 { Vec3::new(v.x, v.y, -v.z) }
fn unity_quat(q: V4) -> Quat { Quat::from_xyzw(-q.x, -q.y, q.z, q.w).normalize() }

pub struct Display { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }

pub struct Calibration {
    pub eye_position: [Vec3; 2],
    /// left X, left Y, right X, right Y — 16 polyvander2d coefficients each
    pub polynomials: [[f32; 16]; 4],
    pub leap_translation: Vec3,
    pub leap_rotation: Quat,
    pub display: Display,
}

impl Calibration {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let json: Esky = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        Ok(Self {
            eye_position: [unity_vec(json.v1.left.eye_position), unity_vec(json.v1.right.eye_position)],
            polynomials: [
                json.v2.left_uv_to_rect_x,
                json.v2.left_uv_to_rect_y,
                json.v2.right_uv_to_rect_x,
                json.v2.right_uv_to_rect_y,
            ],
            leap_translation: unity_vec(json.offsets.leap_translation),
            leap_rotation: unity_quat(json.offsets.leap_rotation),
            display: Display {
                x: json.display.x,
                y: json.display.y,
                width: json.display.width,
                height: json.display.height,
            },
        })
    }

    /// Half-FOV tangents (horizontal, vertical) of the 70° eye camera over the panel's short axis.
    pub fn eye_tangents(&self) -> (f32, f32) {
        let h = (35.0f32).to_radians().tan();
        let (w, hgt) = (self.display.width as f32 / 2.0, self.display.height as f32);
        (h, h * hgt / w)
    }
}
