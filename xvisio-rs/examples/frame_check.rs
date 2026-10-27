//! Compares two pose sources on the device timestamp: raw HID reports recorded by
//! `hid_sniff` (`ts<TAB>hex`) and JSON pose lines (`{"x",..,"q":[..],"tu":..}`, e.g. from
//! xvsdk-bridge). Prints the median translation offset and rotation angle between them.
//!
//!   cargo run --release --example frame_check -- raw.tsv poses.jsonl
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let epoch = std::time::Instant::now();
    let raw: HashMap<u32, xvisio::types::Pose> = std::fs::read_to_string(&args[1])?
        .lines()
        .filter_map(|l| {
            let hex = l.split('\t').nth(1)?;
            let bytes: Vec<u8> = (0..hex.len()).step_by(2).filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect();
            let s = xvisio::protocol::parse_slam_packet(&bytes, epoch)?;
            Some((s.pose.timestamp_us as u32, s.pose))
        })
        .collect();
    let num = |l: &str, key: &str| -> Option<f64> {
        let i = l.find(&format!("\"{key}\":"))? + key.len() + 3;
        l[i..].split([',', '}', ']']).next()?.parse().ok()
    };
    let (mut dt, mut dang) = (Vec::new(), Vec::new());
    for l in std::fs::read_to_string(&args[2])?.lines().filter(|l| l.contains("\"q\"")) {
        let Some(tu) = num(l, "tu") else { continue };
        let Some(r) = raw.get(&(tu as u32)) else { continue };
        let q: Vec<f64> = l[l.find("\"q\":[").unwrap() + 5..].split(']').next().unwrap().split(',').filter_map(|v| v.parse().ok()).collect();
        let t = [num(l, "x").unwrap(), num(l, "y").unwrap(), num(l, "z").unwrap()];
        dt.push((0..3).map(|i| (t[i] - r.translation[i]).powi(2)).sum::<f64>().sqrt());
        let dot = (0..4).map(|i| q[i] * r.quaternion[i]).sum::<f64>().abs().min(1.0);
        dang.push(2.0 * dot.acos().to_degrees());
    }
    let median = |v: &mut Vec<f64>| { v.sort_by(|a, b| a.total_cmp(b)); v.get(v.len() / 2).copied().unwrap_or(f64::NAN) };
    println!("matched {} of {} raw poses", dt.len(), raw.len());
    println!("median |dt| = {:.4} m, median rotation difference = {:.2} deg", median(&mut dt), median(&mut dang));
    Ok(())
}
