use crate::hid::HidTransport;
use crate::protocol::{PID, VID};
use crate::slam::SlamStream;
use crate::types::{DeviceInfo, Features, SlamMode};
use crate::{Result, XvisioError};
use hidapi::HidApi;

/// Check if a hidapi DeviceInfo matches the XR50 HID interface.
/// Interface 3 on Windows/Linux, -1 on macOS IOKit (only HID interface on the device).
fn is_xr50_hid(d: &hidapi::DeviceInfo) -> bool {
    d.vendor_id() == VID
        && d.product_id() == PID
        && (d.interface_number() == 3 || d.interface_number() == -1)
}

/// List all connected XR50 devices with their info.
///
/// Opens each device temporarily to read UUID, version, and features, then closes it.
pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let api = HidApi::new()?;
    let mut devices = Vec::new();

    for dev_info in api.device_list() {
        if !is_xr50_hid(dev_info) {
            continue;
        }

        match query_device_info(&api, dev_info) {
            Ok(info) => devices.push(info),
            Err(e) => {
                log::warn!("Failed to query device at {:?}: {}", dev_info.path(), e);
            }
        }
    }

    Ok(devices)
}

/// Query device info by opening it temporarily.
fn query_device_info(
    api: &HidApi,
    hid_info: &hidapi::DeviceInfo,
) -> Result<DeviceInfo> {
    let device = api.open_path(hid_info.path())?;
    let hid = HidTransport::new(device);
    let uuid = hid.read_uuid()?;
    let version = hid.read_version()?;
    let features = hid.read_features()?;

    Ok(DeviceInfo {
        uuid,
        version,
        features,
        bus_id: hid_info
            .path()
            .to_str()
            .unwrap_or("")
            .to_string(),
        device_address: 0,
    })
}

/// An opened XR50 device ready for queries and SLAM streaming.
pub struct Device {
    /// HidApi keeps the IOKit run loop alive on macOS for commands.
    api: Option<HidApi>,
    hid: Option<HidTransport>,
    /// Path for opening a second handle for SLAM streaming.
    device_path: std::ffi::CString,
    uuid: String,
    version: String,
    features: Features,
}

impl Device {
    /// Open the first available XR50 device.
    pub fn open_first() -> Result<Device> {
        let api = HidApi::new()?;

        let hid_info = api
            .device_list()
            .find(|d| is_xr50_hid(d))
            .ok_or(XvisioError::DeviceNotFound)?;

        let device_path = hid_info.path().to_owned();
        let device = api.open_path(&device_path)?;
        let hid = HidTransport::new(device);

        let uuid = hid.read_uuid()?;
        let version = hid.read_version()?;
        let features = hid.read_features()?;

        log::info!(
            "Opened XR50: UUID={} Version={} Features={:?}",
            uuid,
            version,
            features
        );

        Ok(Device {
            api: Some(api),
            hid: Some(hid),
            device_path,
            uuid,
            version,
            features,
        })
    }

    /// Open a specific device by DeviceInfo.
    pub fn open(info: &DeviceInfo) -> Result<Device> {
        let api = HidApi::new()?;

        let hid_info = api
            .device_list()
            .find(|d| {
                is_xr50_hid(d)
                    && d.path().to_str().unwrap_or("") == info.bus_id
            })
            .ok_or(XvisioError::DeviceNotFound)?;

        let device_path = hid_info.path().to_owned();
        let device = api.open_path(&device_path)?;
        let hid = HidTransport::new(device);

        let uuid = hid.read_uuid()?;
        let version = hid.read_version()?;
        let features = hid.read_features()?;

        log::info!(
            "Opened XR50: UUID={} Version={} Features={:?}",
            uuid,
            version,
            features
        );

        Ok(Device {
            api: Some(api),
            hid: Some(hid),
            device_path,
            uuid,
            version,
            features,
        })
    }

    /// Get the device UUID.
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    /// Get the firmware version string.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the device feature flags.
    pub fn features(&self) -> Features {
        self.features
    }

    /// Send a raw HID command and return the response.
    pub fn hid_command(&self, cmd: &[u8]) -> Result<Vec<u8>> {
        self.hid
            .as_ref()
            .ok_or_else(|| XvisioError::HidCommand("Device handle consumed by SLAM".into()))?
            .transaction(cmd)
    }

    /// Start SLAM streaming in the specified mode.
    ///
    /// On Windows/Linux: uses hidapi for both commands and interrupt reading.
    /// On macOS: closes hidapi handle and uses rusb (libusb) for commands and
    /// interrupt reading, because macOS IOKit can't handle the XR50's USB
    /// re-enumeration during mode changes.
    pub fn start_slam(&mut self, mode: SlamMode) -> Result<SlamStream> {
        let (edge, embedded_algo) = match mode {
            SlamMode::Edge => (true, false),
            SlamMode::Mixed => (true, true),
        };

        if cfg!(target_os = "macos") {
            self.start_slam_rusb(edge, embedded_algo)
        } else {
            self.start_slam_hidapi(edge, embedded_algo)
        }
    }

    /// hidapi-based SLAM start (Windows/Linux).
    fn start_slam_hidapi(&mut self, edge: bool, embedded_algo: bool) -> Result<SlamStream> {
        let hid = self
            .hid
            .as_ref()
            .ok_or_else(|| XvisioError::HidCommand("Device handle already consumed".into()))?;

        hid.configure(edge, embedded_algo)?;

        // Official SDK waits 1 second after configure for device to initialize
        std::thread::sleep(std::time::Duration::from_secs(1));

        hid.edge_stream(true)?;

        // Open a second HID handle for the SLAM reader thread
        let api = HidApi::new()?;
        let slam_device = api.open_path(&self.device_path)?;

        SlamStream::start_hidapi(slam_device, api)
    }

    /// rusb-based SLAM start (macOS).
    ///
    /// macOS requires a special initialization sequence because:
    /// 1. `detach_kernel_driver()` triggers `USBDeviceReEnumerate` (device-wide)
    /// 2. The firmware re-enumerates after configure, invalidating USB handles
    /// 3. Stereo camera init commands are needed (cameras left uninitiated after re-enum)
    ///
    /// The proven sequence (from v14 diagnostic testing):
    /// 1. Precondition: 2x cycles of detach → claim → configure → edge → release
    ///    (clears kernel drivers reliably)
    /// 2. Claim all interfaces WITHOUT detach (tight window after preconditioning)
    /// 3. Configure → stereo camera init → stereo camera start → edge stream
    /// 4. Read interrupt EP 0x83
    fn start_slam_rusb(&mut self, edge: bool, embedded_algo: bool) -> Result<SlamStream> {
        use crate::protocol;

        // Close hidapi handle first — it holds exclusive IOKit access
        drop(self.hid.take());
        drop(self.api.take());

        let timeout = std::time::Duration::from_secs(2);

        // Preconditioning: 2 cycles of detach → claim → configure → edge → release.
        // This reliably clears macOS kernel drivers so the next claim (without detach)
        // succeeds. Each cycle's detach triggers USBDeviceReEnumerate which, combined
        // with the configure command's firmware re-enum, leaves the device in a state
        // where kernel drivers haven't yet reclaimed interfaces.
        for cycle in 1..=2 {
            log::info!("Precondition cycle {}/2: detach → claim → configure → edge → release", cycle);
            match Self::open_rusb_handle_with_detach() {
                Ok(handle) => {
                    // Send configure
                    let cmd = protocol::build_configure_cmd(edge, embedded_algo);
                    let _ = handle.write_control(0x21, 0x09, 0x0202, protocol::HID_INTERFACE as u16, &cmd, timeout);
                    std::thread::sleep(std::time::Duration::from_millis(200));

                    // Send edge stream start
                    let cmd = protocol::build_edge_stream_cmd(true);
                    let _ = handle.write_control(0x21, 0x09, 0x0202, protocol::HID_INTERFACE as u16, &cmd, timeout);
                    std::thread::sleep(std::time::Duration::from_millis(200));

                    // Release — handle drops, device re-enumerates
                    let _ = handle.release_interface(protocol::HID_INTERFACE as u8);
                }
                Err(e) => {
                    log::warn!("Precondition cycle {} failed: {} (continuing)", cycle, e);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        // Wait for device to settle after preconditioning
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Main sequence: claim WITHOUT detach, then send all commands on the same handle.
        // The preconditioning cycles have cleared kernel drivers, so claim should work
        // in the tight window before they re-bind.
        log::info!("Main sequence: claim all interfaces without detach...");
        let handle = Self::open_rusb_handle_no_detach()?;

        // 1. Configure
        log::info!("Sending configure command...");
        let cmd = protocol::build_configure_cmd(edge, embedded_algo);
        match handle.write_control(0x21, 0x09, 0x0202, protocol::HID_INTERFACE as u16, &cmd, timeout) {
            Ok(_) => log::info!("Configure sent"),
            Err(e) => log::warn!("Configure result: {} (continuing)", e),
        }

        // 2. Stereo camera init (required on macOS for non-identity poses)
        log::info!("Sending stereo camera init...");
        let cmd = protocol::build_stereo_camera_init_cmd();
        match handle.write_control(0x21, 0x09, 0x0202, protocol::HID_INTERFACE as u16, &cmd, timeout) {
            Ok(_) => log::info!("Stereo camera init sent"),
            Err(e) => log::warn!("Stereo camera init result: {} (continuing)", e),
        }
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 3. Stereo camera start
        log::info!("Sending stereo camera start...");
        let cmd = protocol::build_stereo_camera_start_cmd();
        match handle.write_control(0x21, 0x09, 0x0202, protocol::HID_INTERFACE as u16, &cmd, timeout) {
            Ok(_) => log::info!("Stereo camera start sent"),
            Err(e) => log::warn!("Stereo camera start result: {} (continuing)", e),
        }
        std::thread::sleep(std::time::Duration::from_millis(500));

        // 4. Edge stream start
        log::info!("Sending edge stream start...");
        let cmd = protocol::build_edge_stream_cmd(true);
        match handle.write_control(0x21, 0x09, 0x0202, protocol::HID_INTERFACE as u16, &cmd, timeout) {
            Ok(_) => log::info!("Edge stream start sent"),
            Err(e) => {
                return Err(XvisioError::HidCommand(format!(
                    "Edge stream command failed: {}", e
                )));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        // Start SLAM reading on the same handle
        SlamStream::start_rusb(handle)
    }

    /// Open XR50 via rusb WITH kernel driver detach. Used for preconditioning cycles.
    /// Retries up to 10 times to handle USB re-enumeration delays.
    fn open_rusb_handle_with_detach() -> Result<rusb::DeviceHandle<rusb::GlobalContext>> {
        use crate::protocol;

        for attempt in 1..=10 {
            let devices = rusb::devices()
                .map_err(|e| XvisioError::HidCommand(format!("rusb enumerate: {}", e)))?;

            let usb_device = match devices.iter().find(|d| {
                d.device_descriptor()
                    .map(|desc| desc.vendor_id() == VID && desc.product_id() == PID)
                    .unwrap_or(false)
            }) {
                Some(d) => d,
                None => {
                    log::info!("XR50 not found (attempt {}), waiting...", attempt);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
            };

            let handle = match usb_device.open() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("rusb open failed (attempt {}): {}", attempt, e);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
            };

            // Detach kernel driver (device-wide on macOS)
            match handle.detach_kernel_driver(protocol::HID_INTERFACE as u8) {
                Ok(_) => log::info!("Detached kernel driver"),
                Err(rusb::Error::NotFound) => {}
                Err(rusb::Error::NotSupported) => {}
                Err(e) => log::warn!("Detach: {} (continuing)", e),
            }

            match handle.claim_interface(protocol::HID_INTERFACE as u8) {
                Ok(_) => {
                    log::info!("Claimed interface {} (attempt {})", protocol::HID_INTERFACE, attempt);
                    return Ok(handle);
                }
                Err(e) => {
                    log::warn!("Claim failed: {} (attempt {})", e, attempt);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
            }
        }

        Err(XvisioError::HidCommand(
            "Failed to open XR50 via rusb after 10 attempts (try sudo)".into(),
        ))
    }

    /// Open XR50 via rusb WITHOUT kernel driver detach. Used for the main SLAM sequence
    /// after preconditioning has cleared kernel drivers. Claims all 4 interfaces to
    /// prevent UVC/HID drivers from reclaiming them.
    /// Retries up to 20 times with short intervals.
    fn open_rusb_handle_no_detach() -> Result<rusb::DeviceHandle<rusb::GlobalContext>> {
        for attempt in 1..=20 {
            let devices = rusb::devices()
                .map_err(|e| XvisioError::HidCommand(format!("rusb enumerate: {}", e)))?;

            let usb_device = match devices.iter().find(|d| {
                d.device_descriptor()
                    .map(|desc| desc.vendor_id() == VID && desc.product_id() == PID)
                    .unwrap_or(false)
            }) {
                Some(d) => d,
                None => {
                    log::info!("XR50 not found (attempt {}), waiting...", attempt);
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    continue;
                }
            };

            let handle = match usb_device.open() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("rusb open failed (attempt {}): {}", attempt, e);
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    continue;
                }
            };

            // Claim all interfaces without detach — prevents kernel drivers from
            // reclaiming during SLAM operation
            let mut all_claimed = true;
            for iface in 0..=3u8 {
                match handle.claim_interface(iface) {
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("Claim interface {} failed: {} (attempt {})", iface, e, attempt);
                        all_claimed = false;
                        break;
                    }
                }
            }

            if all_claimed {
                log::info!("Claimed all interfaces without detach (attempt {})", attempt);
                return Ok(handle);
            }

            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        Err(XvisioError::HidCommand(
            "Failed to claim XR50 interfaces without detach after 20 attempts".into(),
        ))
    }
}
