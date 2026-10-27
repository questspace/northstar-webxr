//! Child processes the hub owns: tracking bridges, the Vite dev server, the native
//! app and the WebXR kiosk browser. Each writes to its own log under `logs/`.
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

pub struct Processes {
    root: PathBuf,
    children: Mutex<BTreeMap<String, Child>>,
}

impl Processes {
    pub fn new(root: PathBuf) -> Self {
        Self { root, children: Mutex::new(BTreeMap::new()) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn log_path(&self, name: &str) -> PathBuf {
        self.root.join("logs").join(format!("{name}.log"))
    }

    pub fn running(&self, name: &str) -> bool {
        let mut children = self.children.lock().unwrap();
        match children.get_mut(name) {
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                _ => {
                    children.remove(name);
                    false
                }
            },
            None => false,
        }
    }

    pub fn start(&self, name: &str, spec: Spec) -> Result<(), String> {
        if self.running(name) {
            return Ok(());
        }
        if !spec.program.exists() {
            return Err(format!("{}: not built ({})", name, spec.program.display()));
        }
        std::fs::create_dir_all(self.root.join("logs")).map_err(|e| e.to_string())?;
        let log = File::create(self.log_path(name)).map_err(|e| e.to_string())?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        let child = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .spawn()
            .map_err(|e| format!("{name}: {e}"))?;
        self.children.lock().unwrap().insert(name.to_string(), child);
        log::info!("started {name}");
        Ok(())
    }

    /// Kills a child and waits up to 5 s for it to exit. The table lock is released first,
    /// so a child that will not die cannot block status requests or other actions.
    pub fn stop(&self, name: &str) {
        let Some(mut child) = self.children.lock().unwrap().remove(name) else { return };
        if let Err(e) = child.kill() {
            log::warn!("kill {name}: {e}");
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                log::info!("stopped {name}");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        log::warn!("{name} (pid {}) did not exit within 5 s", child.id());
    }

    pub fn stop_all(&self) {
        let names: Vec<String> = self.children.lock().unwrap().keys().cloned().collect();
        names.iter().for_each(|n| self.stop(n));
    }

    /// Last `lines` lines of a process log.
    pub fn tail(&self, name: &str, lines: usize) -> String {
        let Ok(text) = std::fs::read_to_string(self.log_path(name)) else { return String::new() };
        let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        all[all.len().saturating_sub(lines)..].join("\n")
    }
}

impl Drop for Processes {
    fn drop(&mut self) {
        self.stop_all();
    }
}
