//! Minimal LeapC binding: just enough to poll tracking frames in HMD mode.
//! Struct layouts mirror LeapC.h (Gemini 5.x); unions are replaced by their
//! largest member, which has the same size and alignment.
#![allow(non_snake_case, dead_code)]

use std::ffi::c_void;

pub type Connection = *mut c_void;

pub const RS_SUCCESS: u32 = 0;
pub const RS_TIMEOUT: u32 = 0xE201_0004;
pub const EVENT_CONNECTION: u32 = 1;
pub const EVENT_CONNECTION_LOST: u32 = 2;
pub const EVENT_POLICY: u32 = 5;
pub const EVENT_TRACKING: u32 = 0x100;
pub const TRACKING_MODE_HMD: u32 = 1;
pub const POLICY_BACKGROUND_FRAMES: u64 = 0x1;
pub const HAND_LEFT: u32 = 0;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Bone {
    pub prev_joint: Vector,
    pub next_joint: Vector,
    pub width: f32,
    pub rotation: Quaternion,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Digit {
    pub finger_id: i32,
    /// metacarpal, proximal, intermediate, distal
    pub bones: [Bone; 4],
    pub is_extended: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Palm {
    pub position: Vector,
    pub stabilized_position: Vector,
    pub velocity: Vector,
    pub normal: Vector,
    pub width: f32,
    pub direction: Vector,
    pub orientation: Quaternion,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Hand {
    pub id: u32,
    pub flags: u32,
    pub hand_type: u32,
    pub confidence: f32,
    pub visible_time: u64,
    pub pinch_distance: f32,
    pub grab_angle: f32,
    pub pinch_strength: f32,
    pub grab_strength: f32,
    pub palm: Palm,
    /// thumb, index, middle, ring, pinky
    pub digits: [Digit; 5],
    pub arm: Bone,
}

#[repr(C, packed)]
pub struct FrameHeader {
    pub reserved: *mut c_void,
    pub frame_id: i64,
    pub timestamp: i64,
}

#[repr(C, packed)]
pub struct TrackingEvent {
    pub info: FrameHeader,
    pub tracking_frame_id: i64,
    pub n_hands: u32,
    pub hands: *const Hand,
    pub framerate: f32,
}

#[repr(C, packed)]
pub struct ConnectionMessage {
    pub size: u32,
    pub event_type: u32,
    pub pointer: *const c_void,
    /// Present since LeapC 5; without it LeapPollConnection writes past the struct.
    pub device_id: u32,
}

extern "system" {
    pub fn LeapCreateConnection(config: *const c_void, out: *mut Connection) -> u32;
    pub fn LeapOpenConnection(connection: Connection) -> u32;
    pub fn LeapPollConnection(connection: Connection, timeout_ms: u32, msg: *mut ConnectionMessage) -> u32;
    pub fn LeapSetTrackingMode(connection: Connection, mode: u32) -> u32;
    pub fn LeapSetPolicyFlags(connection: Connection, set: u64, clear: u64) -> u32;
    pub fn LeapCloseConnection(connection: Connection);
    pub fn LeapDestroyConnection(connection: Connection);
}
