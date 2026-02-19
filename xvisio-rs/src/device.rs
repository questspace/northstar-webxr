use crate::hid::HidTransport;
use crate::protocol::{PID, VID};
use crate::slam::SlamStream;
use crate::types::{DeviceInfo, Features, SlamMode};
use crate::{Result, XvisioError};
use hidapi::HidApi;

/// List all connected XR50 devices with their info.
///
/// Opens each device temporarily to read UUID, version, and features, then closes it.
pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let api = HidApi::new()?;
    let mut devices = Vec::new();

    for dev_info in api.device_list() {
        if dev_info.vendor_id() != VID || dev_info.product_id() != PID {
            continue;
        }
        // Only open interface 3 (usage_page may vary, so filter by interface)
        if dev_info.interface_number() != 3 {
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
    hid: HidTransport,
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

        // Find the HID interface 3 for VID/PID
        let hid_info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == VID
                    && d.product_id() == PID
                    && d.interface_number() == 3
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
            hid,
            device_path,
            uuid,
            version,
            features,
        })
    }

    /// Open a specific device by DeviceInfo.
    pub fn open(info: &DeviceInfo) -> Result<Device> {
        let api = HidApi::new()?;

        // Match by path (stored in bus_id)
        let hid_info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == VID
                    && d.product_id() == PID
                    && d.interface_number() == 3
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
            hid,
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
        self.hid.transaction(cmd)
    }

    /// Start SLAM streaming in the specified mode.
    ///
    /// 1. Sends the configure command
    /// 2. Sends the start edge stream command
    /// 3. Opens a second HID handle for the SLAM reader thread
    /// 4. Returns a `SlamStream` handle for receiving pose data
    pub fn start_slam(&self, mode: SlamMode) -> Result<SlamStream> {
        // Configure device mode
        let (edge, embedded_algo) = match mode {
            SlamMode::Edge => (true, false),
            SlamMode::Mixed => (true, true),
        };
        self.hid.configure(edge, embedded_algo)?;

        // Start edge stream
        self.hid.edge_stream(true)?;

        // Open a second HID handle for the SLAM reader thread
        let api = HidApi::new()?;
        let slam_device = api.open_path(&self.device_path)?;

        // Create SLAM stream with the dedicated HID device
        SlamStream::start(slam_device)
    }
}
