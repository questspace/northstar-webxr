//! Native hot reload: when a source file under `northstar-native/src` changes,
//! rebuild and relaunch the app on the headset. The WebXR side gets this from Vite.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

#[derive(Default)]
pub struct DevState {
    pub enabled: AtomicBool,
    pub building: AtomicBool,
    pub last_result: Mutex<String>,
}

fn newest(dir: &Path) -> SystemTime {
    let mut latest = SystemTime::UNIX_EPOCH;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let t = if path.is_dir() { newest(&path) } else { entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH) };
            latest = latest.max(t);
        }
    }
    latest
}

/// Polls the source tree once a second; `relaunch` runs after each successful build.
pub fn spawn(state: Arc<DevState>, crate_dir: std::path::PathBuf, relaunch: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("native-watch".into())
        .spawn(move || {
            let src = crate_dir.join("src");
            let mut seen = newest(&src);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if !state.enabled.load(Ordering::Relaxed) {
                    seen = newest(&src);
                    continue;
                }
                let now = newest(&src);
                if now <= seen {
                    continue;
                }
                seen = now;
                state.building.store(true, Ordering::Relaxed);
                let output = std::process::Command::new("cargo")
                    .args(["build", "--release"])
                    .current_dir(&crate_dir)
                    .output();
                let result = match output {
                    Ok(o) if o.status.success() => "build ok".to_string(),
                    Ok(o) => {
                        let err = String::from_utf8_lossy(&o.stderr);
                        err.lines().filter(|l| l.starts_with("error") || l.contains("-->")).take(6).collect::<Vec<_>>().join("\n")
                    }
                    Err(e) => format!("cargo: {e}"),
                };
                let ok = result == "build ok";
                *state.last_result.lock().unwrap() = result;
                state.building.store(false, Ordering::Relaxed);
                if ok {
                    relaunch();
                }
            }
        })
        .expect("watch thread");
}
