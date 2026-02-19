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

fn create_hid_api() -> Result<HidApi> {
    let api = HidApi::new()?;
    #[cfg(target_os = "macos")]
    {
        // Keep HID opens shared on macOS to avoid seizing the interface.
        api.set_open_exclusive(false);
    }
    Ok(api)
}

/// List all connected XR50 devices with their info.
///
/// Opens each device temporarily to read UUID, version, and features, then closes it.
pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let api = create_hid_api()?;
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
fn query_device_info(api: &HidApi, hid_info: &hidapi::DeviceInfo) -> Result<DeviceInfo> {
    let device = api.open_path(hid_info.path())?;
    let hid = HidTransport::new(device);
    let uuid = hid.read_uuid()?;
    let version = hid.read_version()?;
    let features = hid.read_features()?;

    Ok(DeviceInfo {
        uuid,
        version,
        features,
        bus_id: hid_info.path().to_str().unwrap_or("").to_string(),
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
        let api = create_hid_api()?;

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
        let api = create_hid_api()?;

        let hid_info = api
            .device_list()
            .find(|d| is_xr50_hid(d) && d.path().to_str().unwrap_or("") == info.bus_id)
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
            SlamMode::Mixed => (false, true),
        };

        if cfg!(target_os = "macos") {
            match Self::read_env_string("XVISIO_MAC_BACKEND", "rusb").as_str() {
                "hidapi" => {
                    log::info!("macOS backend: hidapi");
                    self.start_slam_hidapi(edge, embedded_algo)
                }
                "rusb" => {
                    log::info!("macOS backend: rusb");
                    self.start_slam_rusb(edge, embedded_algo)
                }
                other => {
                    log::warn!(
                        "Unknown XVISIO_MAC_BACKEND='{}', using rusb (supported: rusb|hidapi)",
                        other
                    );
                    self.start_slam_rusb(edge, embedded_algo)
                }
            }
        } else {
            self.start_slam_hidapi(edge, embedded_algo)
        }
    }

    /// hidapi-based SLAM start (Windows/Linux, optional on macOS).
    fn start_slam_hidapi(&mut self, edge: bool, embedded_algo: bool) -> Result<SlamStream> {
        // On macOS, configure frequently causes USB re-enumeration.
        // Re-open and retry edge-start to avoid using a stale HID handle.
        if cfg!(target_os = "macos") {
            return self.start_slam_hidapi_macos(edge, embedded_algo);
        }

        let hid = self
            .hid
            .as_ref()
            .ok_or_else(|| XvisioError::HidCommand("Device handle already consumed".into()))?;

        hid.configure(edge, embedded_algo)?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        hid.edge_stream(edge)?;

        // Open a second HID handle for the SLAM reader thread.
        let api = create_hid_api()?;
        let slam_device = api.open_path(&self.device_path)?;
        SlamStream::start_hidapi(slam_device, api)
    }

    fn start_slam_hidapi_macos(&mut self, edge: bool, embedded_algo: bool) -> Result<SlamStream> {
        let uvc_mode = Self::read_env_u8("XVISIO_UVC_MODE", 1);
        let rotation_enabled = Self::read_env_bool("XVISIO_ROTATION_ENABLED", true);
        let enable_stereo_init = Self::read_env_bool("XVISIO_ENABLE_STEREO_INIT", false);
        let reopen_after_config = Self::read_env_bool("XVISIO_REOPEN_AFTER_CONFIG", true);
        let reconnect_attempts = 40usize;
        let reconnect_delay = std::time::Duration::from_millis(100);
        log::info!(
            "macOS hidapi params: uvcMode={} rotationEnabled={} stereoInit={} reopenAfterConfig={}",
            uvc_mode,
            rotation_enabled,
            enable_stereo_init,
            reopen_after_config
        );

        {
            let hid = self
                .hid
                .as_ref()
                .ok_or_else(|| XvisioError::HidCommand("Device handle already consumed".into()))?;
            hid.configure_with_uvc(edge, uvc_mode, embedded_algo)?;
        }

        // Same delay as the official flow after configure.
        std::thread::sleep(std::time::Duration::from_secs(1));

        if reopen_after_config {
            self.reopen_hid_handle(reconnect_attempts, reconnect_delay)?;
        }

        if enable_stereo_init {
            let mut last_err: Option<XvisioError> = None;
            let mut stereo_init_ok = false;
            for attempt in 1..=reconnect_attempts {
                let res = {
                    let hid = self.hid.as_ref().ok_or_else(|| {
                        XvisioError::HidCommand("Device handle already consumed".into())
                    })?;
                    hid.stereo_camera_init()
                };
                match res {
                    Ok(_) => {
                        stereo_init_ok = true;
                        break;
                    }
                    Err(e) => {
                        let msg = e.to_string().to_ascii_lowercase();
                        last_err = Some(e);
                        if msg.contains("disconnected")
                            || msg.contains("no such device")
                            || msg.contains("not found")
                        {
                            log::warn!(
                                "hidapi stereo init retry {}/{} after reconnect",
                                attempt,
                                reconnect_attempts
                            );
                            std::thread::sleep(reconnect_delay);
                            self.reopen_hid_handle(reconnect_attempts, reconnect_delay)?;
                            continue;
                        }
                        return Err(last_err.unwrap());
                    }
                }
            }
            if !stereo_init_ok {
                return Err(last_err.unwrap_or_else(|| {
                    XvisioError::HidCommand("hidapi stereo init failed after retries".into())
                }));
            }

            std::thread::sleep(std::time::Duration::from_millis(50));

            let mut last_err: Option<XvisioError> = None;
            let mut stereo_start_ok = false;
            for attempt in 1..=reconnect_attempts {
                let res = {
                    let hid = self.hid.as_ref().ok_or_else(|| {
                        XvisioError::HidCommand("Device handle already consumed".into())
                    })?;
                    hid.stereo_camera_start()
                };
                match res {
                    Ok(_) => {
                        stereo_start_ok = true;
                        break;
                    }
                    Err(e) => {
                        let msg = e.to_string().to_ascii_lowercase();
                        last_err = Some(e);
                        if msg.contains("disconnected")
                            || msg.contains("no such device")
                            || msg.contains("not found")
                        {
                            log::warn!(
                                "hidapi stereo start retry {}/{} after reconnect",
                                attempt,
                                reconnect_attempts
                            );
                            std::thread::sleep(reconnect_delay);
                            self.reopen_hid_handle(reconnect_attempts, reconnect_delay)?;
                            continue;
                        }
                        return Err(last_err.unwrap());
                    }
                }
            }
            if !stereo_start_ok {
                return Err(last_err.unwrap_or_else(|| {
                    XvisioError::HidCommand("hidapi stereo start failed after retries".into())
                }));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        // Edge start can still race re-enumeration; retry with reopen on disconnect.
        let mut last_err: Option<XvisioError> = None;
        for attempt in 1..=reconnect_attempts {
            let edge_result = {
                let hid = self.hid.as_ref().ok_or_else(|| {
                    XvisioError::HidCommand("Device handle already consumed".into())
                })?;
                hid.edge_stream_with_params(if edge { 1 } else { 0 }, rotation_enabled, false)
            };

            match edge_result {
                Ok(_) => {
                    log::info!(
                        "hidapi edge stream start succeeded (attempt {}, edgeMode={})",
                        attempt,
                        if edge { 1 } else { 0 }
                    );
                    let hid = self.hid.take().ok_or_else(|| {
                        XvisioError::HidCommand("Device handle already consumed".into())
                    })?;
                    let api = self
                        .api
                        .take()
                        .ok_or_else(|| XvisioError::HidCommand("HidApi context consumed".into()))?;
                    return SlamStream::start_hidapi(hid.into_device(), api);
                }
                Err(e) => {
                    let msg = e.to_string().to_ascii_lowercase();
                    last_err = Some(e);
                    if msg.contains("disconnected")
                        || msg.contains("no such device")
                        || msg.contains("not found")
                    {
                        log::warn!(
                            "hidapi edge stream start retry {}/{} after reconnect",
                            attempt,
                            reconnect_attempts
                        );
                        std::thread::sleep(reconnect_delay);
                        self.reopen_hid_handle(reconnect_attempts, reconnect_delay)?;
                        continue;
                    }
                    return Err(last_err.unwrap());
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            XvisioError::HidCommand("hidapi edge stream start failed after retries".into())
        }))
    }

    /// rusb-based SLAM start (macOS).
    ///
    /// macOS requires rusb/libusb for SLAM mode transitions.
    ///
    /// Default behavior mirrors the Windows/Linux command sequence:
    /// 1. claim interface(s)
    /// 2. configure
    /// 3. edge stream start
    /// 4. read interrupt EP 0x83
    ///
    /// Extra recovery knobs are available via env vars for unstable setups:
    /// - `XVISIO_PRECONDITION_CYCLES`
    /// - `XVISIO_ENABLE_STEREO_INIT`
    fn start_slam_rusb(&mut self, edge: bool, embedded_algo: bool) -> Result<SlamStream> {
        use crate::protocol;

        // Close hidapi handle first — it holds exclusive IOKit access
        drop(self.hid.take());
        drop(self.api.take());

        let timeout = std::time::Duration::from_secs(2);
        // Keep macOS defaults aligned with the known-good Windows/Linux path:
        // configure(edge=1, uvcMode=0, embeddedAlgo=0), then edge stream
        // with rotationEnabled=true.
        let uvc_mode = Self::read_env_u8("XVISIO_UVC_MODE", 0);
        let rotation_enabled = Self::read_env_bool("XVISIO_ROTATION_ENABLED", true);
        let claim_all_interfaces = Self::read_env_bool("XVISIO_CLAIM_ALL_INTERFACES", false);
        let precondition_cycles = Self::read_env_u8("XVISIO_PRECONDITION_CYCLES", 0) as usize;
        let enable_stereo_init = Self::read_env_bool("XVISIO_ENABLE_STEREO_INIT", false);
        let reopen_after_config = Self::read_env_bool("XVISIO_REOPEN_AFTER_CONFIG", true);
        let reopen_after_edge_start = Self::read_env_bool("XVISIO_REOPEN_AFTER_EDGE_START", false);
        let allow_detach_fallback = Self::read_env_bool("XVISIO_ALLOW_DETACH_FALLBACK", true);
        log::info!(
            "macOS SLAM params: uvcMode={} rotationEnabled={} claimAllIfaces={} preconditionCycles={} stereoInit={} reopenAfterConfig={} reopenAfterEdgeStart={} detachFallback={}",
            uvc_mode,
            rotation_enabled,
            claim_all_interfaces,
            precondition_cycles,
            enable_stereo_init,
            reopen_after_config,
            reopen_after_edge_start,
            allow_detach_fallback,
        );

        // Preconditioning: 2 cycles of detach → claim → configure → edge → release.
        // This reliably clears macOS kernel drivers so the next claim (without detach)
        // succeeds. Each cycle's detach triggers USBDeviceReEnumerate which, combined
        // with the configure command's firmware re-enum, leaves the device in a state
        // where kernel drivers haven't yet reclaimed interfaces.
        for cycle in 1..=precondition_cycles {
            log::info!(
                "Precondition cycle {}/{}: detach → claim → configure → edge → release",
                cycle,
                precondition_cycles
            );
            match Self::open_rusb_handle_with_detach() {
                Ok(handle) => {
                    // Send configure
                    let cmd = protocol::build_configure_cmd_with_uvc(edge, uvc_mode, embedded_algo);
                    let _ = Self::send_hid_command_rusb(
                        &handle,
                        &cmd,
                        protocol::CMD_CONFIGURE,
                        timeout,
                        "precondition configure",
                    );
                    std::thread::sleep(std::time::Duration::from_millis(200));

                    // Send edge stream start
                    let cmd = protocol::build_edge_stream_cmd_with_params(
                        if edge { 1 } else { 0 },
                        rotation_enabled,
                        false,
                    );
                    let _ = Self::send_hid_command_rusb(
                        &handle,
                        &cmd,
                        protocol::CMD_EDGE_STREAM,
                        timeout,
                        "precondition edge start",
                    );
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

        if precondition_cycles > 0 {
            // Wait for device to settle after preconditioning
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        // Main sequence: claim WITHOUT detach, then send all commands on the same handle.
        // The preconditioning cycles have cleared kernel drivers, so claim should work
        // in the tight window before they re-bind.
        log::info!("Main sequence: claim interfaces without detach...");
        let mut handle =
            Self::open_rusb_handle_no_detach(claim_all_interfaces, allow_detach_fallback)?;

        // 1. Configure
        log::info!("Sending configure command...");
        let cmd = protocol::build_configure_cmd_with_uvc(edge, uvc_mode, embedded_algo);
        Self::send_hid_command_rusb(&handle, &cmd, protocol::CMD_CONFIGURE, timeout, "configure")?;
        std::thread::sleep(std::time::Duration::from_secs(1));

        // On some macOS setups, configure triggers a USB re-enumeration and invalidates
        // the current handle. Re-open proactively before sending follow-up commands.
        if reopen_after_config {
            drop(handle);
            std::thread::sleep(std::time::Duration::from_millis(200));
            log::info!("Re-opening handle after configure...");
            handle = Self::open_rusb_handle_no_detach(claim_all_interfaces, allow_detach_fallback)?;
        }

        if enable_stereo_init {
            // 2. Stereo camera init (required on some macOS setups for non-identity poses)
            log::info!("Sending stereo camera init...");
            let cmd = protocol::build_stereo_camera_init_cmd();
            match Self::send_hid_command_rusb(
                &handle,
                &cmd,
                protocol::CMD_STEREO_CAMERA_INIT,
                timeout,
                "stereo camera init",
            ) {
                Ok(_) => log::info!("Stereo camera init sent"),
                Err(e) => log::warn!("Stereo camera init failed: {} (continuing)", e),
            }
            std::thread::sleep(std::time::Duration::from_millis(50));

            // 3. Stereo camera start
            log::info!("Sending stereo camera start...");
            let cmd = protocol::build_stereo_camera_start_cmd();
            match Self::send_hid_command_rusb(
                &handle,
                &cmd,
                protocol::CMD_STEREO_CAMERA_START,
                timeout,
                "stereo camera start",
            ) {
                Ok(_) => log::info!("Stereo camera start sent"),
                Err(e) => log::warn!("Stereo camera start failed: {} (continuing)", e),
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        } else {
            log::info!("Skipping stereo camera init/start (XVISIO_ENABLE_STEREO_INIT=0)");
        }

        // 4. Edge stream start
        log::info!("Sending edge stream start...");
        let cmd = protocol::build_edge_stream_cmd_with_params(
            if edge { 1 } else { 0 },
            rotation_enabled,
            false,
        );
        Self::send_hid_command_rusb(
            &handle,
            &cmd,
            protocol::CMD_EDGE_STREAM,
            timeout,
            "edge stream start",
        )?;
        log::info!("Edge stream start sent");

        std::thread::sleep(std::time::Duration::from_millis(300));

        // Some macOS runs re-enumerate again right after edge stream enable.
        // Optional re-open avoids starting the reader on a stale handle.
        if reopen_after_edge_start {
            drop(handle);
            std::thread::sleep(std::time::Duration::from_millis(200));
            log::info!("Re-opening handle after edge stream start...");
            handle = Self::open_rusb_handle_no_detach(claim_all_interfaces, allow_detach_fallback)?;
        }

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
                    log::info!(
                        "Claimed interface {} (attempt {})",
                        protocol::HID_INTERFACE,
                        attempt
                    );
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
    /// after preconditioning has cleared kernel drivers.
    /// By default claims all interfaces [3,1,2,0], which is more robust on macOS.
    /// Set `XVISIO_CLAIM_ALL_INTERFACES=0` to prefer interface 3 first.
    /// Retries up to 20 times with short intervals.
    fn open_rusb_handle_no_detach(
        claim_all_interfaces: bool,
        allow_detach_fallback: bool,
    ) -> Result<rusb::DeviceHandle<rusb::GlobalContext>> {
        use crate::protocol;

        const IFACES_HID: &[u8] = &[3];
        const IFACES_ALL: &[u8] = &[3, 1, 2, 0];
        let interface_sets: &[&[u8]] = if claim_all_interfaces {
            &[IFACES_ALL]
        } else {
            &[IFACES_HID, IFACES_ALL]
        };

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

            for interfaces in interface_sets {
                let handle = match usb_device.open() {
                    Ok(h) => h,
                    Err(e) => {
                        log::warn!("rusb open failed (attempt {}): {}", attempt, e);
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        continue;
                    }
                };

                // Claim interface 3 only by default (libxvisio-compatible).
                // Fallback to claim-all can help in tight re-enumeration windows.
                let mut all_claimed = true;
                for &iface in *interfaces {
                    match handle.claim_interface(iface) {
                        Ok(_) => {}
                        Err(rusb::Error::Access)
                            if iface == protocol::HID_INTERFACE && allow_detach_fallback =>
                        {
                            // Last-resort fallback: if kernel HID re-bound before claim,
                            // detach and retry once for interface 3.
                            match handle.detach_kernel_driver(iface) {
                                Ok(_)
                                | Err(rusb::Error::NotFound)
                                | Err(rusb::Error::NotSupported) => {}
                                Err(e) => {
                                    log::warn!(
                                        "Detach fallback on interface {} failed: {} (attempt {})",
                                        iface,
                                        e,
                                        attempt
                                    );
                                }
                            }
                            match handle.claim_interface(iface) {
                                Ok(_) => log::info!(
                                    "Claimed interface {} after detach fallback (attempt {})",
                                    iface,
                                    attempt
                                ),
                                Err(e) => {
                                    log::warn!(
                                        "Claim interface {} failed after detach fallback: {} (attempt {})",
                                        iface,
                                        e,
                                        attempt
                                    );
                                    all_claimed = false;
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "Claim interface {} failed: {} (attempt {})",
                                iface,
                                e,
                                attempt
                            );
                            all_claimed = false;
                            break;
                        }
                    }
                }

                if all_claimed {
                    log::info!(
                        "Claimed interfaces {:?} without detach (attempt {})",
                        interfaces,
                        attempt
                    );
                    return Ok(handle);
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        Err(XvisioError::HidCommand(
            "Failed to claim XR50 interfaces without detach after 20 attempts".into(),
        ))
    }

    fn send_hid_command_rusb(
        handle: &rusb::DeviceHandle<rusb::GlobalContext>,
        cmd: &[u8; crate::protocol::REPORT_SIZE],
        expected_echo: &[u8],
        timeout: std::time::Duration,
        label: &str,
    ) -> Result<()> {
        use crate::protocol;

        handle
            .write_control(
                0x21,
                0x09,
                0x0202,
                protocol::HID_INTERFACE as u16,
                cmd,
                timeout,
            )
            .map_err(|e| XvisioError::HidCommand(format!("{} write failed: {}", label, e)))?;

        let mut response = [0u8; protocol::REPORT_SIZE];
        match handle.read_control(
            0xA1,
            0x01,
            0x0101,
            protocol::HID_INTERFACE as u16,
            &mut response,
            timeout,
        ) {
            Ok(len) => {
                if len < 1 + expected_echo.len() {
                    log::warn!("{} ack too short ({} bytes)", label, len);
                } else if response[0] != protocol::PREFIX_DEVICE_TO_HOST
                    || &response[1..1 + expected_echo.len()] != expected_echo
                {
                    log::warn!(
                        "{} ack mismatch: prefix=0x{:02x} echo={:02x?}",
                        label,
                        response[0],
                        &response[1..1 + expected_echo.len()],
                    );
                }
            }
            Err(e) => {
                log::warn!("{} GET_REPORT failed: {} (continuing)", label, e);
            }
        }

        Ok(())
    }

    fn reopen_hid_handle(&mut self, attempts: usize, delay: std::time::Duration) -> Result<()> {
        drop(self.hid.take());
        drop(self.api.take());

        for attempt in 1..=attempts {
            let api = create_hid_api()?;
            let hid_info = match api.device_list().find(|d| is_xr50_hid(d)) {
                Some(d) => d,
                None => {
                    if attempt <= 5 || attempt % 10 == 0 {
                        log::info!("XR50 HID not found (attempt {})", attempt);
                    }
                    std::thread::sleep(delay);
                    continue;
                }
            };

            let path = hid_info.path().to_owned();
            match api.open_path(&path) {
                Ok(device) => {
                    self.device_path = path;
                    self.hid = Some(HidTransport::new(device));
                    self.api = Some(api);
                    if attempt > 1 {
                        log::info!("Re-opened HID handle (attempt {})", attempt);
                    }
                    return Ok(());
                }
                Err(e) => {
                    if attempt <= 5 || attempt % 10 == 0 {
                        log::warn!("Failed to open XR50 HID (attempt {}): {}", attempt, e);
                    }
                    std::thread::sleep(delay);
                }
            }
        }

        Err(XvisioError::HidCommand(format!(
            "Failed to re-open XR50 HID handle after {} attempts",
            attempts
        )))
    }

    fn read_env_bool(name: &str, default: bool) -> bool {
        std::env::var(name)
            .ok()
            .and_then(|v| {
                let v = v.trim().to_ascii_lowercase();
                match v.as_str() {
                    "1" | "true" | "yes" | "on" => Some(true),
                    "0" | "false" | "no" | "off" => Some(false),
                    _ => None,
                }
            })
            .unwrap_or(default)
    }

    fn read_env_u8(name: &str, default: u8) -> u8 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.trim().parse::<u8>().ok())
            .unwrap_or(default)
    }

    fn read_env_string(name: &str, default: &str) -> String {
        std::env::var(name)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| default.to_string())
    }
}
