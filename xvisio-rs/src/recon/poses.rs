//! Recent SLAM poses stamped on arrival, to find the pose a camera frame was captured at.
use crate::types::Pose;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Camera capture to frame arrival, fitted on recorded scans (`examples/recon_eval.rs`).
pub const FRAME_DELAY: Duration = Duration::from_millis(60);

/// About two seconds of edge SLAM at ~1 kHz.
const CAPACITY: usize = 2048;

/// Shared between the SLAM reader (push) and the mapper (lookup).
#[derive(Clone, Default)]
pub struct PoseHistory(Arc<Mutex<VecDeque<(Instant, Pose)>>>);

impl PoseHistory {
    pub fn push(&self, pose: Pose) {
        let mut q = self.0.lock().unwrap();
        q.push_back((Instant::now(), pose));
        if q.len() > CAPACITY {
            q.pop_front();
        }
    }

    /// The pose that arrived nearest to `t`.
    pub fn at(&self, t: Instant) -> Option<Pose> {
        let q = self.0.lock().unwrap();
        q.iter().min_by_key(|(at, _)| at.max(&t).duration_since(*at.min(&t))).map(|(_, p)| *p)
    }
}
