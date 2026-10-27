//! Passive HID sniffer: opens the XR50's HID interface without sending anything and logs
//! every input report as `device_ts_us<TAB>hex`. On Windows every open handle receives its
//! own copy of input reports, so this runs next to another owner (the official SDK, the
//! bridge) and records exactly what it sees.
//!
//!   cargo run --release --example hid_sniff -- [seconds] > reports.tsv
use std::io::Write;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let api = hidapi::HidApi::new()?;
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == xvisio::protocol::VID && d.product_id() == xvisio::protocol::PID && d.interface_number() == 3)
        .ok_or("XR50 HID interface not found")?;
    let device = api.open_path(info.path())?;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut buf = [0u8; 64];
    let (start, mut count) = (Instant::now(), 0u64);
    while start.elapsed() < Duration::from_secs(seconds) {
        let n = device.read_timeout(&mut buf, 200)?;
        if n < 7 {
            continue;
        }
        let ts = u32::from_le_bytes([buf[3], buf[4], buf[5], buf[6]]);
        let hex: String = buf[..n].iter().map(|b| format!("{b:02x}")).collect();
        writeln!(out, "{ts}\t{hex}")?;
        count += 1;
    }
    out.flush()?;
    eprintln!("{count} reports in {seconds} s");
    Ok(())
}
