//! Diagnostic: on one live frame, counts left-right-consistent matches for every combination
//! of calibration order and frame-half assignment. The right combination stands out.
use xvisio::camera::StereoCamera;
use xvisio::stereo::{Calibration, StereoConfig, StereoMatcher};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "../calibration/xr50-fisheye.json".into());
    let calibration = Calibration::from_json(&std::fs::read_to_string(path)?)?;
    let mut swapped = calibration.clone();
    swapped.cameras.swap(0, 1);
    let config = StereoConfig { speckle_size: 0, uniqueness: 1.1, ..StereoConfig::default() };

    let mut device = xvisio::Device::open_first()?;
    let _slam = device.start_slam_with_cameras(xvisio::SlamMode::Edge)?;
    let mut camera = StereoCamera::open()?;
    for _ in 0..10 {
        camera.frame()?; // let exposure settle
    }
    let f = camera.frame()?;
    for (name, cal) in [("calibration order", &calibration), ("cameras swapped", &swapped)] {
        let m = StereoMatcher::new(cal, (640, 400), config);
        for (halves, a, b) in [("top=cam0", 0, 1), ("top=cam1", 1, 0)] {
            let (_, _, depth) = m.depth_from(&f.images[a], &f.images[b]);
            let n = depth.iter().filter(|d| d.is_finite()).count();
            println!("{name:18} {halves}: left camera {} -> {n} matches", m.left);
        }
    }
    Ok(())
}
