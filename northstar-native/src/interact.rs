//! Hands as the interface, native: pinch to grab a cube (one hand moves and
//! turns it, two hands scale it), index-fingertip presses on a floating button
//! panel, and depth-only hand occluders so holograms behind the real hand are
//! cut out. Same rules as `visual-test/src/xr/interaction.ts`.
use crate::render::{transform, Instance, Kind};
use crate::tracking::HandSample;
use crate::transforms::LeapToHead;
use glam::{Mat4, Quat, Vec3};

const PINCH_START: f32 = 0.02;
const PINCH_END: f32 = 0.035;
const REACH: f32 = 0.03;
const PRESS_TRAVEL: f32 = 0.01;
const PRESS_HOVER: f32 = 0.05;
const BUTTON: Vec3 = Vec3::new(0.09, 0.036, 0.012);
const JOINT_RADIUS: f32 = 0.011;

/// `HandSample.joints`: palm, wrist, then carp..tip for each digit.
const DIGIT_BASE: usize = 2;
const JOINTS_PER_DIGIT: usize = 5;
const THUMB_TIP: usize = DIGIT_BASE + 4;
const INDEX_TIP: usize = DIGIT_BASE + JOINTS_PER_DIGIT + 4;

pub struct Object {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    pub color: Vec3,
}

impl Object {
    fn world(&self) -> Mat4 { transform(self.position, self.rotation, Vec3::splat(self.scale)) }
    fn set_world(&mut self, world: Mat4) {
        let (scale, rotation, position) = world.to_scale_rotation_translation();
        self.scale = scale.x;
        self.rotation = rotation;
        self.position = position;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action { Reset }

pub struct Button {
    pub position: Vec3,
    pub color: Vec3,
    pub action: Action,
    push: f32,
    armed: bool,
}

impl Button {
    pub fn new(position: Vec3, color: Vec3, action: Action) -> Self {
        Self { position, color, action, push: 0.0, armed: true }
    }
}

/// One tracked hand in the world frame.
pub struct Hand {
    pub joints: Vec<Vec3>,
    pub index: Vec3,
    pub thumb: Vec3,
    pub pinch: Vec3,
    pub orientation: Quat,
}

pub type Hands = [Option<Hand>; 2];

struct Grab {
    object: usize,
    /// grab frame⁻¹ · object world (single hand)
    offset: Mat4,
}

/// Both hands on one object: scale and turn it about the point between them.
struct Pair {
    object: usize,
    d0: Vec3,
    m0: Vec3,
    world0: Mat4,
}

pub struct Interaction {
    pub objects: Vec<Object>,
    pub buttons: Vec<Button>,
    pinching: [bool; 2],
    pressing: [bool; 2],
    grabs: [Option<Grab>; 2],
    pair: Option<Pair>,
}

fn grab_frame(hand: &Hand) -> Mat4 { Mat4::from_rotation_translation(hand.orientation, hand.pinch) }

/// A cube between two points, used for the occluder's bones.
fn bone(a: Vec3, b: Vec3, radius: f32) -> Instance {
    let d = b - a;
    let rotation = Quat::from_rotation_arc(Vec3::X, d.normalize_or_zero());
    Instance::new(transform((a + b) * 0.5, rotation, Vec3::new(d.length(), radius * 2.0, radius * 2.0)), Vec3::ZERO, false)
}

/// Leap samples → world-frame hands in fixed slots (left, right).
pub fn hands(samples: &[HandSample], leap: &LeapToHead, rig: Mat4) -> Hands {
    let (_, rig_rotation, _) = rig.to_scale_rotation_translation();
    let mut hands = [None, None];
    for sample in samples {
        if sample.joints.len() < DIGIT_BASE + 5 * JOINTS_PER_DIGIT {
            continue;
        }
        let joints: Vec<Vec3> = sample.joints.iter().map(|&mm| rig.transform_point3(leap.point(mm))).collect();
        let (index, thumb) = (joints[INDEX_TIP], joints[THUMB_TIP]);
        hands[if sample.left { 0 } else { 1 }] = Some(Hand {
            index,
            thumb,
            pinch: (index + thumb) * 0.5,
            orientation: rig_rotation * leap.rotate(sample.palm_orientation),
            joints,
        });
    }
    hands
}

impl Interaction {
    pub fn new(objects: Vec<Object>, buttons: Vec<Button>) -> Self {
        Self { objects, buttons, pinching: [false; 2], pressing: [false; 2], grabs: [None, None], pair: None }
    }

    /// Advances grabs and presses for this frame; returns the buttons that clicked.
    pub fn update(&mut self, hands: &Hands) -> Vec<Action> {
        let mut actions = Vec::new();
        for button in &mut self.buttons {
            button.push = 0.0;
        }
        for slot in 0..2 {
            let Some(hand) = &hands[slot] else {
                self.release(slot, hands);
                self.pinching[slot] = false;
                continue;
            };
            let distance = hand.index.distance(hand.thumb);
            self.pinching[slot] = if self.pinching[slot] { distance < PINCH_END } else { distance < PINCH_START };

            if self.pinching[slot] && self.grabs[slot].is_none() && !self.pressing[slot] {
                if let Some(object) = self.nearest(hand.pinch) {
                    self.start_grab(slot, object, hands);
                }
            } else if !self.pinching[slot] && self.grabs[slot].is_some() {
                self.release(slot, hands);
            }
            if self.grabs[slot].is_some() {
                self.apply_grab(slot, hands);
                self.pressing[slot] = false;
            } else {
                self.pressing[slot] = self.press(hand, &mut actions);
            }
        }
        actions
    }

    fn nearest(&self, point: Vec3) -> Option<usize> {
        self.objects
            .iter()
            .enumerate()
            .map(|(i, o)| (i, o.position.distance(point) - o.scale * 0.87))
            .filter(|(_, d)| *d < REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn start_grab(&mut self, slot: usize, object: usize, hands: &Hands) {
        let hand = hands[slot].as_ref().unwrap();
        let world = self.objects[object].world();
        self.grabs[slot] = Some(Grab { object, offset: grab_frame(hand).inverse() * world });
        if let Some(other) = hands[1 - slot].as_ref().filter(|_| self.grabs[1 - slot].as_ref().is_some_and(|g| g.object == object)) {
            self.pair = Some(Pair { object, d0: hand.pinch - other.pinch, m0: (hand.pinch + other.pinch) * 0.5, world0: world });
        }
    }

    fn release(&mut self, slot: usize, hands: &Hands) {
        let Some(grab) = self.grabs[slot].take() else { return };
        if self.pair.as_ref().is_some_and(|p| p.object == grab.object) {
            self.pair = None;
            // The other hand carries on alone from where the object is now.
            if let (Some(other), Some(hand)) = (self.grabs[1 - slot].as_mut(), hands[1 - slot].as_ref()) {
                other.offset = grab_frame(hand).inverse() * self.objects[grab.object].world();
            }
        }
    }

    fn apply_grab(&mut self, slot: usize, hands: &Hands) {
        let hand = hands[slot].as_ref().unwrap();
        let grab = self.grabs[slot].as_ref().unwrap();
        let world = match (&self.pair, hands[1 - slot].as_ref()) {
            (Some(pair), Some(other)) if pair.object == grab.object => {
                let d = hand.pinch - other.pinch;
                let mid = (hand.pinch + other.pinch) * 0.5;
                let scale = d.length() / pair.d0.length().max(1e-4);
                let rotation = Quat::from_rotation_arc(pair.d0.normalize(), d.normalize());
                // T(mid) · R · S · T(-m0) · world0
                Mat4::from_translation(mid) * Mat4::from_quat(rotation) * Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-pair.m0) * pair.world0
            }
            _ => grab_frame(hand) * grab.offset,
        };
        self.objects[grab.object].set_world(world);
    }

    /// Index fingertip against every button face; true while it is on one.
    fn press(&mut self, hand: &Hand, actions: &mut Vec<Action>) -> bool {
        let mut pressing = false;
        for button in &mut self.buttons {
            let local = hand.index - button.position;
            let on_face = local.x.abs() <= BUTTON.x / 2.0 && local.y.abs() <= BUTTON.y / 2.0;
            let dz = local.z - BUTTON.z / 2.0;
            let depth = if on_face && dz < PRESS_HOVER && dz > -PRESS_TRAVEL - 0.02 { (-dz / PRESS_TRAVEL).clamp(0.0, 1.0) } else { 0.0 };
            button.push = button.push.max(depth);
            pressing |= depth > 0.0;
            if depth >= 1.0 && button.armed {
                button.armed = false;
                actions.push(button.action);
            } else if depth < 0.5 {
                button.armed = true;
            }
        }
        pressing
    }

    /// Draw list: occluders first, then the button panel, the cubes and the fingertip cursors.
    pub fn draws(&self, hands: &Hands, toggled: impl Fn(Action) -> bool) -> Vec<(Kind, Vec<Instance>)> {
        let mut occluders = Vec::new();
        let mut cursors = Vec::new();
        for (slot, hand) in hands.iter().enumerate() {
            let Some(hand) = hand else { continue };
            let j = &hand.joints;
            for digit in 0..5 {
                let base = DIGIT_BASE + digit * JOINTS_PER_DIGIT;
                occluders.push(bone(j[1], j[base], JOINT_RADIUS));
                for k in base..base + JOINTS_PER_DIGIT - 1 {
                    occluders.push(bone(j[k], j[k + 1], JOINT_RADIUS));
                    occluders.push(Instance::new(transform(j[k + 1], Quat::IDENTITY, Vec3::splat(JOINT_RADIUS * 2.0)), Vec3::ZERO, false));
                }
            }
            occluders.push(Instance::new(transform(j[0], hand.orientation, Vec3::new(0.075, 0.03, 0.085)), Vec3::ZERO, false));
            let pinching = self.pinching[slot];
            let (at, size, color) = if pinching {
                (hand.pinch, 0.007, Vec3::new(1.0, 0.7, 0.28))
            } else {
                (hand.index, 0.0045, Vec3::new(0.56, 0.89, 1.0))
            };
            cursors.push(Instance::new(transform(at, Quat::IDENTITY, Vec3::splat(size)), color, false));
        }
        let mut panel = Vec::new();
        for button in &self.buttons {
            let color = if toggled(button.action) { Vec3::new(0.22, 0.63, 0.41) } else { button.color };
            let lit = if button.push > 0.0 { color * 1.4 } else { color };
            panel.push(Instance::new(transform(button.position - Vec3::Z * 0.006, Quat::IDENTITY, Vec3::new(BUTTON.x, BUTTON.y, 0.006)), Vec3::new(0.1, 0.13, 0.19), true));
            panel.push(Instance::new(
                transform(button.position - Vec3::Z * (button.push * PRESS_TRAVEL), Quat::IDENTITY, Vec3::new(BUTTON.x - 0.004, BUTTON.y - 0.004, BUTTON.z)),
                lit,
                true,
            ));
        }
        let cubes = self
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let held = self.grabs.iter().flatten().any(|g| g.object == i);
                Instance::new(o.world(), if held { o.color * 1.5 } else { o.color }, true)
            })
            .collect();
        vec![(Kind::Occluder, occluders), (Kind::Cube, panel), (Kind::Cube, cubes), (Kind::Sphere, cursors)]
    }
}
