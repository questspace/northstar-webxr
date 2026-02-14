//! macOS SLAM diagnostic v14: stereo camera init + extended data inspection.
//!
//! v13 proved: stable 978 Hz reads for 37s, but ALL identity poses.
//! The SLAM algorithm runs but cameras appear inactive.
//!
//! Hypotheses to test:
//! 1. Stereo cameras need explicit init after USB re-enumeration
//! 2. The extended data (bytes 37-62) might reveal sensor/IMU state
//!
//! Tests:
//! A. Send stereo camera init {0xfe,0x20,0x21} + start {0xfe,0x20,0x22} before edge stream
//! B. Read with extended data dump to check IMU/accelerometer
//! C. Try with rotationEnabled=true (C++ libxvisio uses this, our Rust code uses false)
//!
//! Usage: sudo cargo run --release --example macos_diag

use std::time::{Duration, Instant};

const VID: u16 = 0x040E;
const PID: u16 = 0xF408;
const HID_INTERFACE: u8 = 3;
const SLAM_ENDPOINT: u8 = 0x83;
const SCALE: f64 = 6.103515625e-05;

fn find_and_open(timeout_secs: u64, interval_ms: u64) -> Option<(rusb::DeviceHandle<rusb::GlobalContext>, u8, u8)> {
    let start = Instant::now();
    let mut attempt = 0u32;
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        attempt += 1;
        let devices = match rusb::devices() { Ok(d) => d, Err(_) => continue };
        if let Some(dev) = devices.iter().find(|d| {
            d.device_descriptor()
                .map(|desc| desc.vendor_id() == VID && desc.product_id() == PID)
                .unwrap_or(false)
        }) {
            if let Ok(h) = dev.open() {
                let bus = dev.bus_number();
                let addr = dev.address();
                if attempt <= 3 || attempt % 10 == 0 {
                    println!("    Found: bus {} addr {} ({:.1}s, attempt {})",
                        bus, addr, start.elapsed().as_secs_f64(), attempt);
                }
                return Some((h, bus, addr));
            }
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
    None
}

fn send_cmd(handle: &rusb::DeviceHandle<rusb::GlobalContext>, cmd: &[u8; 63], label: &str) -> bool {
    match handle.write_control(0x21, 0x09, 0x0202, HID_INTERFACE as u16, cmd, Duration::from_secs(2)) {
        Ok(n) => { println!("    {} OK ({} bytes)", label, n); true }
        Err(e) => { println!("    {} FAILED: {}", label, e); false }
    }
}

fn build_cmd(bytes: &[u8]) -> [u8; 63] {
    let mut cmd = [0u8; 63];
    cmd[0] = 0x02;
    let len = bytes.len().min(62);
    cmd[1..1 + len].copy_from_slice(&bytes[..len]);
    cmd
}

/// Detach + claim all interfaces cycle (for preconditioning)
fn detach_claim_release(handle: &rusb::DeviceHandle<rusb::GlobalContext>) {
    handle.detach_kernel_driver(HID_INTERFACE).ok();
    for i in 0..=3u8 { handle.claim_interface(i).ok(); }
    for i in 0..=3u8 { handle.release_interface(i).ok(); }
}

/// Read SLAM with extended data inspection
fn read_slam_extended(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    max_seconds: u64,
    label: &str,
) -> (u64, u64) {
    println!("    Reading SLAM for {}s [{}] (MOVE DEVICE!)...", max_seconds, label);
    let start = Instant::now();
    let mut count = 0u64;
    let mut tracking = 0u64;
    let mut errors = 0u64;
    let mut buf = [0u8; 64];

    while start.elapsed() < Duration::from_secs(max_seconds) {
        match handle.read_interrupt(SLAM_ENDPOINT, &mut buf, Duration::from_millis(200)) {
            Ok(n) if n >= 27 => {
                count += 1;
                let base = if buf[0] == 0x01 && buf[1] == 0xA2 && buf[2] == 0x33 { 7usize }
                           else if buf[0] == 0xA2 && buf[1] == 0x33 { 6usize }
                           else { continue };
                if n < base + 20 { continue; }

                let tx = i32::from_le_bytes([buf[base], buf[base+1], buf[base+2], buf[base+3]]) as f64 * SCALE;
                let ty = i32::from_le_bytes([buf[base+4], buf[base+5], buf[base+6], buf[base+7]]) as f64 * SCALE;
                let tz = i32::from_le_bytes([buf[base+8], buf[base+9], buf[base+10], buf[base+11]]) as f64 * SCALE;
                let qw = i16::from_le_bytes([buf[base+12], buf[base+13]]) as f64 * SCALE;
                let qx = i16::from_le_bytes([buf[base+14], buf[base+15]]) as f64 * SCALE;
                let qy = i16::from_le_bytes([buf[base+16], buf[base+17]]) as f64 * SCALE;
                let qz = i16::from_le_bytes([buf[base+18], buf[base+19]]) as f64 * SCALE;

                let is_tracking = tx.abs() > 1e-6 || ty.abs() > 1e-6 || tz.abs() > 1e-6
                    || (qw - 1.0).abs() > 0.01 || qx.abs() > 0.01 || qy.abs() > 0.01 || qz.abs() > 0.01;
                if is_tracking { tracking += 1; }

                // Print first 3 packets with FULL raw dump + extended data
                if count <= 3 {
                    print!("    pkt#{} raw: ", count);
                    for i in 0..n.min(63) { print!("{:02x}", buf[i]); if i == 2 || i == 6 || i == 18 || i == 26 || i == 36 { print!(" "); } }
                    println!();

                    let tag = if is_tracking { "TRACKING!" } else { "identity" };
                    println!("      pos=[{:+.4},{:+.4},{:+.4}] q=[w{:.4},x{:.4},y{:.4},z{:.4}] [{}]",
                        tx, ty, tz, qw, qx, qy, qz, tag);

                    // Extended data: IMU + confidence
                    if n >= 49 {
                        let ax = i16::from_le_bytes([buf[37], buf[38]]) as f64 * SCALE;
                        let ay = i16::from_le_bytes([buf[39], buf[40]]) as f64 * SCALE;
                        let az = i16::from_le_bytes([buf[41], buf[42]]) as f64 * SCALE;
                        let gx = i16::from_le_bytes([buf[43], buf[44]]) as f64 * SCALE;
                        let gy = i16::from_le_bytes([buf[45], buf[46]]) as f64 * SCALE;
                        let gz = i16::from_le_bytes([buf[47], buf[48]]) as f64 * SCALE;
                        println!("      accel=[{:+.4},{:+.4},{:+.4}] gyro=[{:+.4},{:+.4},{:+.4}]",
                            ax, ay, az, gx, gy, gz);
                        if n >= 59 {
                            let conf = i16::from_le_bytes([buf[57], buf[58]]) as f64 * SCALE;
                            println!("      confidence={:.4} raw_57_58=[{:02x}{:02x}]", conf, buf[57], buf[58]);
                        }
                    }
                }

                // Periodic summary
                if count % 5000 == 0 {
                    let tag = if is_tracking { "TRACKING" } else { "identity" };
                    println!("    #{} ({:.0} Hz): pos=[{:+.4},{:+.4},{:+.4}] q=[w{:.4}] [{}]",
                        count, count as f64 / start.elapsed().as_secs_f64(), tx, ty, tz, qw, tag);
                }

                if is_tracking && tracking <= 5 {
                    println!("    >>> TRACKING at #{}: pos=[{:+.4},{:+.4},{:+.4}] q=[w{:.4},x{:.4},y{:.4},z{:.4}]",
                        count, tx, ty, tz, qw, qx, qy, qz);
                }
            }
            Ok(_) => {} // short packet
            Err(rusb::Error::Timeout) => {}
            Err(rusb::Error::Pipe) => {
                errors += 1;
                handle.clear_halt(SLAM_ENDPOINT).ok();
                if errors > 50 { break; }
            }
            Err(_) => {
                errors += 1;
                if errors > 50 { break; }
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("    => {} pkts ({:.0} Hz), {} tracking, {} errors [{:.0}s]",
        count, count as f64 / elapsed.max(0.001), tracking, errors, elapsed);
    (count, tracking)
}

fn main() {
    env_logger::init();
    println!("=== XR50 macOS SLAM v14 — camera init + extended data ===\n");
    println!("MOVE THE DEVICE CONTINUOUSLY!\n");

    let mut total_tracking = 0u64;

    // =========================================================================
    // STEP 1: Precondition — detach+claim+release twice to clear kernel drivers
    //         (reproduces the Session 3→4→5 pattern from v13)
    // =========================================================================
    println!("=== STEP 1: Preconditioning (detach+claim+release x2) ===");
    for round in 1..=2 {
        if let Some((handle, _, _)) = find_and_open(5, 300) {
            println!("  Round {}: detach + claim all + configure + release", round);
            detach_claim_release(&handle);
            // Re-open after detach re-enum
            std::thread::sleep(Duration::from_millis(500));
            if let Some((h2, _, _)) = find_and_open(5, 100) {
                h2.detach_kernel_driver(HID_INTERFACE).ok();
                for i in 0..=3u8 { h2.claim_interface(i).ok(); }
                let cfg = build_cmd(&[0x19, 0x95, 0x01, 0x00, 0x00]);
                send_cmd(&h2, &cfg, &format!("Configure round {}", round));
                let edge = build_cmd(&[0xA2, 0x33, 0x01, 0x00, 0x00]);
                send_cmd(&h2, &edge, &format!("Edge stream round {}", round));
                for i in 0..=3u8 { h2.release_interface(i).ok(); }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    println!("\n  Brief pause...");
    std::thread::sleep(Duration::from_millis(300));

    // =========================================================================
    // STEP 2: Claim WITHOUT detach (like v13 Session 5)
    // =========================================================================
    println!("\n=== STEP 2: Claim all without detach ===");
    let handle = match find_and_open(5, 50) {
        Some((h, bus, addr)) => {
            println!("  Device: bus {} addr {}", bus, addr);
            let mut ok = true;
            for i in [3u8, 1, 2, 0] {
                match h.claim_interface(i) {
                    Ok(_) => print!("  +iface{}", i),
                    Err(e) => { print!("  -iface{}({})", i, e); if i == 3 { ok = false; } }
                }
            }
            println!();
            if !ok {
                println!("  FAILED to claim iface 3 — try unplugging for 60s and re-running");
                return;
            }
            h
        }
        None => {
            println!("  Device not found");
            return;
        }
    };

    // =========================================================================
    // TEST A: Baseline — edge stream only (no camera init)
    //         Expected: identity poses (same as v13)
    // =========================================================================
    println!("\n=== TEST A: Edge stream only (baseline, rotationEnabled=false) ===");
    {
        let edge = build_cmd(&[0xA2, 0x33, 0x01, 0x00, 0x00]);
        send_cmd(&handle, &edge, "Edge stream start");
        std::thread::sleep(Duration::from_millis(100));
        let (_, t) = read_slam_extended(&handle, 8, "baseline");
        total_tracking += t;
        // Stop edge stream
        let stop = build_cmd(&[0xA2, 0x33, 0x00, 0x00, 0x00]);
        send_cmd(&handle, &stop, "Edge stream stop");
        std::thread::sleep(Duration::from_millis(200));
    }

    // =========================================================================
    // TEST B: Edge stream with rotationEnabled=true
    //         C++ libxvisio uses rotationEnabled=true, our Rust uses false
    // =========================================================================
    println!("\n=== TEST B: Edge stream with rotationEnabled=true ===");
    {
        // Re-configure (edge=1, uvc=0, algo=0)
        let cfg = build_cmd(&[0x19, 0x95, 0x01, 0x00, 0x00]);
        send_cmd(&handle, &cfg, "Re-configure(1,0,0)");
        std::thread::sleep(Duration::from_millis(500));
        // Edge stream with rotationEnabled=true (byte 4 = 0x01)
        let edge = build_cmd(&[0xA2, 0x33, 0x01, 0x01, 0x00]);
        send_cmd(&handle, &edge, "Edge stream (rotation=true)");
        std::thread::sleep(Duration::from_millis(100));
        let (_, t) = read_slam_extended(&handle, 8, "rotation=true");
        total_tracking += t;
        let stop = build_cmd(&[0xA2, 0x33, 0x00, 0x00, 0x00]);
        send_cmd(&handle, &stop, "Edge stream stop");
        std::thread::sleep(Duration::from_millis(200));
    }

    // =========================================================================
    // TEST C: Stereo camera init commands before edge stream
    //         {0xfe, 0x20, 0x21} = stereo camera init
    //         {0xfe, 0x20, 0x22} = stereo camera start streaming
    // =========================================================================
    println!("\n=== TEST C: Stereo camera init + edge stream ===");
    {
        // Send stereo camera init
        let cam_init = build_cmd(&[0xfe, 0x20, 0x21]);
        send_cmd(&handle, &cam_init, "Stereo camera init");
        std::thread::sleep(Duration::from_millis(50));

        // Send stereo camera start streaming
        let cam_start = build_cmd(&[0xfe, 0x20, 0x22]);
        send_cmd(&handle, &cam_start, "Stereo camera start");
        std::thread::sleep(Duration::from_millis(100));

        // Configure + edge stream
        let cfg = build_cmd(&[0x19, 0x95, 0x01, 0x00, 0x00]);
        send_cmd(&handle, &cfg, "Configure(1,0,0)");
        std::thread::sleep(Duration::from_millis(500));
        let edge = build_cmd(&[0xA2, 0x33, 0x01, 0x01, 0x00]);
        send_cmd(&handle, &edge, "Edge stream (rotation=true)");
        std::thread::sleep(Duration::from_millis(100));
        let (_, t) = read_slam_extended(&handle, 10, "camera init");
        total_tracking += t;
        let stop = build_cmd(&[0xA2, 0x33, 0x00, 0x00, 0x00]);
        send_cmd(&handle, &stop, "Edge stream stop");
        std::thread::sleep(Duration::from_millis(200));
    }

    // =========================================================================
    // TEST D: Camera init AFTER configure (maybe order matters)
    // =========================================================================
    println!("\n=== TEST D: Configure → camera init → edge stream ===");
    {
        let cfg = build_cmd(&[0x19, 0x95, 0x01, 0x00, 0x00]);
        send_cmd(&handle, &cfg, "Configure(1,0,0)");
        std::thread::sleep(Duration::from_millis(500));

        let cam_init = build_cmd(&[0xfe, 0x20, 0x21]);
        send_cmd(&handle, &cam_init, "Stereo camera init");
        std::thread::sleep(Duration::from_millis(50));

        let cam_start = build_cmd(&[0xfe, 0x20, 0x22]);
        send_cmd(&handle, &cam_start, "Stereo camera start");
        std::thread::sleep(Duration::from_millis(100));

        let edge = build_cmd(&[0xA2, 0x33, 0x01, 0x01, 0x00]);
        send_cmd(&handle, &edge, "Edge stream (rotation=true)");
        std::thread::sleep(Duration::from_millis(100));
        let (_, t) = read_slam_extended(&handle, 10, "cam after cfg");
        total_tracking += t;
        let stop = build_cmd(&[0xA2, 0x33, 0x00, 0x00, 0x00]);
        send_cmd(&handle, &stop, "Edge stream stop");
        std::thread::sleep(Duration::from_millis(200));
    }

    // =========================================================================
    // TEST E: Try uvcMode=1 (PROTOCOL.md says Edge SLAM uses uvcMode=1,
    //         but MEMORY.md says "NOT 1". Test both to see difference.)
    // =========================================================================
    println!("\n=== TEST E: Configure with uvcMode=1 ===");
    {
        // configure: edge=1, uvcMode=1, algo=0
        let cfg = build_cmd(&[0x19, 0x95, 0x01, 0x01, 0x00]);
        send_cmd(&handle, &cfg, "Configure(1,1,0) [uvcMode=1]");
        std::thread::sleep(Duration::from_millis(500));

        let edge = build_cmd(&[0xA2, 0x33, 0x01, 0x01, 0x00]);
        send_cmd(&handle, &edge, "Edge stream (rotation=true)");
        std::thread::sleep(Duration::from_millis(100));
        let (_, t) = read_slam_extended(&handle, 10, "uvcMode=1");
        total_tracking += t;
        let stop = build_cmd(&[0xA2, 0x33, 0x00, 0x00, 0x00]);
        send_cmd(&handle, &stop, "Edge stream stop");
    }

    // Cleanup
    for i in 0..=3u8 { handle.release_interface(i).ok(); }

    // =========================================================================
    // Summary
    // =========================================================================
    println!("\n=== SUMMARY ===");
    println!("  Total tracking packets across all tests: {}", total_tracking);
    if total_tracking > 0 {
        println!("  >>> SOME TEST PRODUCED TRACKING DATA! <<<");
    } else {
        println!("  >>> ALL IDENTITY across all tests <<<");
        println!("  The XR50 SLAM cameras are inactive after macOS USB re-enumeration.");
        println!("  None of the camera init commands or parameter changes helped.");
        println!();
        println!("  Remaining options:");
        println!("    1. IOKit direct USB access (avoid detach/re-enum entirely)");
        println!("    2. Codeless kext to prevent macOS drivers from binding");
        println!("    3. macOS SLAM support limited to Windows/Linux only");
    }
}
