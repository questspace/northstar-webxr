//! Top-level windows on the panel that do not belong there: when the headset
//! re-enumerates, Windows parks whatever was last on that area back onto it,
//! and a browser or editor ends up invisible behind the combiners.
#![cfg(windows)]

use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

type EnumProc = extern "system" fn(hwnd: *mut c_void, lparam: isize) -> i32;

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: EnumProc, lparam: isize) -> i32;
    fn IsWindowVisible(hwnd: *mut c_void) -> i32;
    fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
    fn GetWindowTextW(hwnd: *mut c_void, text: *mut u16, max: i32) -> i32;
    fn SetWindowPos(hwnd: *mut c_void, after: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
}

const SWP_NOSIZE: u32 = 0x1;
const SWP_NOZORDER: u32 = 0x4;
const SWP_NOACTIVATE: u32 = 0x10;

/// Titles of the windows that are meant to be on the panel.
const OURS: &[&str] = &["NorthStar", "Vibestar", "Seeing Spaces", "Hand menu", "WebXR"];

/// True when the hub runs with administrator rights (service restarts need no UAC then).
pub fn elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

extern "system" fn collect(hwnd: *mut c_void, lparam: isize) -> i32 {
    let out = unsafe { &mut *(lparam as *mut Vec<(*mut c_void, Rect, String)>) };
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    let mut rect = Rect::default();
    let mut title = [0u16; 256];
    let n = unsafe {
        GetWindowRect(hwnd, &mut rect);
        GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32)
    };
    if n > 0 {
        out.push((hwnd, rect, String::from_utf16_lossy(&title[..n as usize])));
    }
    1
}

/// Moves every titled window whose centre lies on the panel, except ours, to `to` (laptop
/// desktop coordinates). Returns the titles moved.
pub fn sweep(panel: (i32, i32, i32, i32), to: (i32, i32)) -> Vec<String> {
    let mut windows: Vec<(*mut c_void, Rect, String)> = Vec::new();
    unsafe { EnumWindows(collect, &mut windows as *mut _ as isize) };
    let (px, py, pw, ph) = panel;
    windows
        .into_iter()
        .filter(|(_, r, title)| {
            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            cx >= px && cx < px + pw && cy >= py && cy < py + ph && !OURS.iter().any(|o| title.contains(o))
        })
        .map(|(hwnd, _, title)| {
            unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), to.0, to.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
            title
        })
        .collect()
}
