//! USB presence of the headset's trackers and the Ultraleap service state.
use serde::Serialize;

const XVISIO_VID: u16 = 0x040E;
const ULTRALEAP_VID: u16 = 0x2936;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Usb {
    pub xr50: bool,
    pub leap: bool,
}

pub fn usb() -> Usb {
    let mut usb = Usb::default();
    if let Ok(devices) = rusb::devices() {
        for d in devices.iter() {
            if let Ok(desc) = d.device_descriptor() {
                match desc.vendor_id() {
                    XVISIO_VID => usb.xr50 = true,
                    ULTRALEAP_VID => usb.leap = true,
                    _ => {}
                }
            }
        }
    }
    usb
}

/// "Running" / "Stopped" / ... from `sc query`, or "?" when unavailable.
pub fn leap_service_state() -> String {
    let Ok(out) = std::process::Command::new("sc").args(["query", "UltraleapTracking"]).output() else { return "?".into() };
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find(|l| l.contains("STATE"))
        .and_then(|l| l.split_whitespace().last())
        .map(|s| {
            let s = s.to_lowercase();
            format!("{}{}", &s[..1].to_uppercase(), &s[1..])
        })
        .unwrap_or_else(|| "?".into())
}

/// Runs a PowerShell snippet with administrator rights: directly when the hub itself is
/// elevated, otherwise through a UAC prompt. Waits for it.
fn elevated(script: &str) -> Result<(), String> {
    let status = if crate::windows::elevated() {
        std::process::Command::new("powershell").args(["-NoProfile", "-Command", script]).status()
    } else {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &format!("Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile','-Command',\"{script}\"")])
            .status()
    }
    .map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| "elevated command failed or was declined".into())
}

/// Clears a hung service process and starts the service.
pub fn restart_leap_service() -> Result<(), String> {
    elevated("Get-Process -Name LeapSvc -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Seconds 2; Start-Service -Name UltraleapTracking")
}

/// Software re-plug of the XR50: after a USB re-enumeration it can answer with an
/// empty identity until its USB device is cycled.
pub fn reset_xr50_usb() -> Result<(), String> {
    elevated("$d = Get-PnpDevice -PresentOnly | Where-Object InstanceId -like 'USB\\VID_040E&PID_F408\\*'; $d | Disable-PnpDevice -Confirm:$false; Start-Sleep -Seconds 3; $d | Enable-PnpDevice -Confirm:$false")
}
