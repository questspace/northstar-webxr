//! Windows display layout: find the NorthStar panel and keep the desktop sane
//! (laptop primary at the origin, panel to its right in its native mode).
#![cfg(windows)]

use serde::Serialize;

#[repr(C)]
#[derive(Clone, Copy)]
struct DisplayDeviceW {
    cb: u32,
    device_name: [u16; 32],
    device_string: [u16; 128],
    state_flags: u32,
    device_id: [u16; 128],
    device_key: [u16; 128],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DevModeW {
    device_name: [u16; 32],
    spec_version: u16,
    driver_version: u16,
    size: u16,
    driver_extra: u16,
    fields: u32,
    position_x: i32,
    position_y: i32,
    display_orientation: u32,
    display_fixed_output: u32,
    color: i16,
    duplex: i16,
    y_resolution: i16,
    tt_option: i16,
    collate: i16,
    form_name: [u16; 32],
    log_pixels: u16,
    bits_per_pel: u32,
    pels_width: u32,
    pels_height: u32,
    display_flags: u32,
    display_frequency: u32,
    icm_method: u32,
    icm_intent: u32,
    media_type: u32,
    dither_type: u32,
    reserved1: u32,
    reserved2: u32,
    panning_width: u32,
    panning_height: u32,
}

#[link(name = "user32")]
extern "system" {
    fn EnumDisplayDevicesW(device: *const u16, index: u32, out: *mut DisplayDeviceW, flags: u32) -> i32;
    fn EnumDisplaySettingsExW(device: *const u16, mode: u32, out: *mut DevModeW, flags: u32) -> i32;
    fn ChangeDisplaySettingsExW(device: *const u16, mode: *const DevModeW, hwnd: *mut core::ffi::c_void, flags: u32, param: *mut core::ffi::c_void) -> i32;
}

const ATTACHED_TO_DESKTOP: u32 = 0x1;
const PRIMARY_DEVICE: u32 = 0x4;
const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 0x1;
const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
const DM_POSITION: u32 = 0x20;
const DM_BITSPERPEL: u32 = 0x4_0000;
const DM_PELSWIDTH: u32 = 0x8_0000;
const DM_PELSHEIGHT: u32 = 0x10_0000;
const DM_DISPLAYFREQUENCY: u32 = 0x40_0000;
const DM_DISPLAYORIENTATION: u32 = 0x80;

/// How the panel is mounted relative to a desk monitor: 0, 1, 2, 3 = 0°, 90°, 180°, 270° (DMDO_*).
/// Windows forgets this whenever the panel re-enumerates as a new monitor, so the hub re-applies it.
/// Override with NS_PANEL_ROTATION. This NorthStar wants 180° (verified on the head 2026-10-26: with
/// 0° the warp no longer matches the optics and every head motion is inverted).
pub fn panel_orientation() -> u32 {
    std::env::var("NS_PANEL_ROTATION").ok().and_then(|v| v.parse().ok()).unwrap_or(2)
}
const CDS_UPDATEREGISTRY: u32 = 0x1;
const CDS_NORESET: u32 = 0x1000_0000;
const CDS_SET_PRIMARY: u32 = 0x10;

/// EDID product id of the NorthStar display board (`DISPLAY\OPD2C02`, "SF05_21022301").
const PANEL_EDID: &str = "OPD2C02";

#[derive(Serialize, Clone, Debug)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub hz: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct Display {
    pub name: String,
    pub adapter: String,
    pub monitor: String,
    pub monitor_id: String,
    pub attached: bool,
    pub primary: bool,
    pub is_panel: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    /// DMDO_* value: 0, 1, 2, 3 = 0°, 90°, 180°, 270°
    pub orientation: u32,
    pub modes: Vec<Mode>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn narrow(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}

fn device(index: u32) -> Option<DisplayDeviceW> {
    let mut dd: DisplayDeviceW = unsafe { std::mem::zeroed() };
    dd.cb = std::mem::size_of::<DisplayDeviceW>() as u32;
    (unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut dd, 0) } != 0).then_some(dd)
}

fn monitor_of(adapter: &[u16]) -> Option<DisplayDeviceW> {
    let mut dd: DisplayDeviceW = unsafe { std::mem::zeroed() };
    dd.cb = std::mem::size_of::<DisplayDeviceW>() as u32;
    (unsafe { EnumDisplayDevicesW(adapter.as_ptr(), 0, &mut dd, EDD_GET_DEVICE_INTERFACE_NAME) } != 0).then_some(dd)
}

fn devmode(name: &[u16], index: u32) -> Option<DevModeW> {
    let mut dm: DevModeW = unsafe { std::mem::zeroed() };
    dm.size = std::mem::size_of::<DevModeW>() as u16;
    (unsafe { EnumDisplaySettingsExW(name.as_ptr(), index, &mut dm, 0) } != 0).then_some(dm)
}

/// Every display adapter output that has a monitor, attached to the desktop or not.
pub fn list() -> Vec<Display> {
    let mut out = Vec::new();
    for i in 0.. {
        let Some(dd) = device(i) else { break };
        let name = narrow(&dd.device_name);
        let Some(mon) = monitor_of(&dd.device_name) else { continue };
        let monitor_id = narrow(&mon.device_id);
        let modes: Vec<Mode> = (0..)
            .map_while(|k| devmode(&dd.device_name, k))
            .map(|m| Mode { width: m.pels_width, height: m.pels_height, hz: m.display_frequency })
            .collect();
        let current = devmode(&dd.device_name, ENUM_CURRENT_SETTINGS);
        out.push(Display {
            name,
            adapter: narrow(&dd.device_string),
            monitor: narrow(&mon.device_string),
            is_panel: monitor_id.to_uppercase().contains(PANEL_EDID),
            monitor_id,
            attached: dd.state_flags & ATTACHED_TO_DESKTOP != 0,
            primary: dd.state_flags & PRIMARY_DEVICE != 0,
            x: current.map_or(0, |m| m.position_x),
            y: current.map_or(0, |m| m.position_y),
            width: current.map_or(0, |m| m.pels_width),
            height: current.map_or(0, |m| m.pels_height),
            hz: current.map_or(0, |m| m.display_frequency),
            orientation: current.map_or(0, |m| m.display_orientation),
            modes,
        });
    }
    out
}

/// The panel's native mode: the largest one, at its highest refresh rate.
pub fn native_mode(panel: &Display) -> Option<Mode> {
    panel.modes.iter().max_by_key(|m| (m.width * m.height, m.hz)).cloned()
}

/// True when the layout is already what we want.
pub fn layout_ok(displays: &[Display]) -> bool {
    let Some(panel) = displays.iter().find(|d| d.is_panel) else { return true };
    let Some(laptop) = displays.iter().find(|d| !d.is_panel && d.attached) else { return true };
    let native = native_mode(panel);
    panel.attached
        && !panel.primary
        && laptop.primary
        && laptop.x == 0
        && laptop.y == 0
        && panel.x == laptop.width as i32
        && panel.orientation == panel_orientation()
        && native.map_or(true, |m| m.width == panel.width && m.height == panel.height && m.hz == panel.hz)
}

/// Laptop primary at (0,0) in its current mode; panel attached to its right in native mode.
pub fn fix_layout() -> Result<String, String> {
    let displays = list();
    let panel = displays.iter().find(|d| d.is_panel).ok_or("no NorthStar panel detected (EDID OPD2C02)")?;
    let laptop = displays.iter().find(|d| !d.is_panel && (d.attached || d.primary)).ok_or("no laptop display found")?;
    let mode = native_mode(panel).ok_or("panel reports no modes")?;

    let laptop_name = wide(&laptop.name);
    let mut lap = devmode(&laptop_name, ENUM_CURRENT_SETTINGS).ok_or("laptop current mode unavailable")?;
    lap.position_x = 0;
    lap.position_y = 0;
    lap.fields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL | DM_DISPLAYFREQUENCY;
    let rc = unsafe { ChangeDisplaySettingsExW(laptop_name.as_ptr(), &lap, std::ptr::null_mut(), CDS_SET_PRIMARY | CDS_UPDATEREGISTRY | CDS_NORESET, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(format!("laptop primary: ChangeDisplaySettingsEx returned {rc}"));
    }

    let panel_name = wide(&panel.name);
    let mut pan = devmode(&panel_name, ENUM_CURRENT_SETTINGS)
        .or_else(|| devmode(&panel_name, 0))
        .ok_or("panel mode unavailable")?;
    pan.position_x = lap.pels_width as i32;
    pan.position_y = 0;
    pan.pels_width = mode.width;
    pan.pels_height = mode.height;
    pan.display_frequency = mode.hz;
    pan.bits_per_pel = 32;
    pan.display_orientation = panel_orientation();
    pan.fields = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL | DM_DISPLAYFREQUENCY | DM_DISPLAYORIENTATION;
    let rc = unsafe { ChangeDisplaySettingsExW(panel_name.as_ptr(), &pan, std::ptr::null_mut(), CDS_UPDATEREGISTRY | CDS_NORESET, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(format!("panel mode: ChangeDisplaySettingsEx returned {rc}"));
    }

    let rc = unsafe { ChangeDisplaySettingsExW(std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), 0, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(format!("apply: ChangeDisplaySettingsEx returned {rc}"));
    }
    Ok(format!("laptop {} primary at 0,0; panel {} at {},0 in {}x{}@{} rotation {}°", laptop.name, panel.name, lap.pels_width, mode.width, mode.height, mode.hz, panel_orientation() * 90))
}
