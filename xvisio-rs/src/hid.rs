use crate::protocol::{self, PREFIX_DEVICE_TO_HOST, REPORT_SIZE};
use crate::{Result, XvisioError};
use hidapi::HidDevice;

/// HID transport layer using hidapi for SET_REPORT / GET_REPORT.
///
/// On Windows, hidapi's `write()` uses byte[0] as the HID report ID.
/// The XR50 protocol prefix 0x02 (host-to-device) doubles as the output
/// report ID, so `build_command()` output (63 bytes starting with 0x02)
/// can be passed directly to `write()`.
pub struct HidTransport {
    device: HidDevice,
}

impl HidTransport {
    pub fn new(device: HidDevice) -> Self {
        Self { device }
    }

    /// Consume the transport and return the inner HID device handle.
    /// Used on macOS where exclusive access prevents opening a second handle.
    pub fn into_device(self) -> HidDevice {
        self.device
    }

    /// Send a HID command and receive the response.
    ///
    /// 1. Builds a 63-byte buffer: [0x02, cmd_bytes..., padding]
    /// 2. Sends via `write()` — byte[0]=0x02 serves as both report ID and protocol prefix
    /// 3. Reads via `get_input_report()` — report ID 0x01 = device-to-host prefix
    /// 4. Validates response prefix and command echo
    pub fn transaction(&self, cmd: &[u8]) -> Result<Vec<u8>> {
        // build_command returns [0x02, cmd..., padding] (63 bytes)
        // hidapi uses byte[0]=0x02 as report ID — which matches our protocol prefix
        let send_buf = protocol::build_command(cmd);

        self.device
            .write(&send_buf)
            .map_err(|e| XvisioError::HidCommand(format!("write failed: {}", e)))?;

        // Small delay to let device process the command
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Read input report (report ID 0x01 = device-to-host prefix)
        let mut recv_buf = [0u8; REPORT_SIZE + 1];
        recv_buf[0] = PREFIX_DEVICE_TO_HOST; // report ID = 0x01
        let len = self
            .device
            .get_input_report(&mut recv_buf)
            .map_err(|e| XvisioError::HidCommand(format!("get_input_report failed: {}", e)))?;

        let response = recv_buf[..len].to_vec();

        // Validate response prefix
        if response.is_empty() || response[0] != PREFIX_DEVICE_TO_HOST {
            return Err(XvisioError::InvalidResponse(
                response.first().copied().unwrap_or(0),
            ));
        }

        Ok(response)
    }

    /// Read UUID string from the device.
    pub fn read_uuid(&self) -> Result<String> {
        let response = self.transaction(protocol::CMD_UUID)?;
        let offset = protocol::validate_response(&response, protocol::CMD_UUID)?;
        Ok(protocol::extract_string(&response[offset..]))
    }

    /// Read firmware version string from the device.
    pub fn read_version(&self) -> Result<String> {
        let response = self.transaction(protocol::CMD_VERSION)?;
        let offset = protocol::validate_response(&response, protocol::CMD_VERSION)?;
        Ok(protocol::extract_string(&response[offset..]))
    }

    /// Read features bitmap from the device.
    pub fn read_features(&self) -> Result<crate::types::Features> {
        let response = self.transaction(protocol::CMD_FEATURES)?;
        let offset = protocol::validate_response(&response, protocol::CMD_FEATURES)?;
        Ok(protocol::parse_features(&response[offset..]))
    }

    /// Send the configure command for the given SLAM mode and UVC mode.
    pub fn configure_with_uvc(&self, edge: bool, uvc_mode: u8, embedded_algo: bool) -> Result<()> {
        let cmd_buf = protocol::build_configure_cmd_with_uvc(edge, uvc_mode, embedded_algo);

        self.device
            .write(&cmd_buf)
            .map_err(|e| XvisioError::HidCommand(format!("Configure write failed: {}", e)))?;

        std::thread::sleep(std::time::Duration::from_millis(20));

        // Read response (may be all zeros, that's OK)
        let mut recv_buf = [0u8; REPORT_SIZE + 1];
        recv_buf[0] = PREFIX_DEVICE_TO_HOST;
        let _ = self.device.get_input_report(&mut recv_buf);

        Ok(())
    }

    /// Send the configure command for the given SLAM mode.
    pub fn configure(&self, edge: bool, embedded_algo: bool) -> Result<()> {
        self.configure_with_uvc(edge, 0, embedded_algo)
    }

    /// Send the edge stream command with explicit parameters.
    pub fn edge_stream_with_params(
        &self,
        edge_mode: u8,
        rotation_enabled: bool,
        flipped: bool,
    ) -> Result<()> {
        let cmd_buf =
            protocol::build_edge_stream_cmd_with_params(edge_mode, rotation_enabled, flipped);

        self.device
            .write(&cmd_buf)
            .map_err(|e| XvisioError::HidCommand(format!("Edge stream cmd failed: {}", e)))?;

        std::thread::sleep(std::time::Duration::from_millis(20));

        let mut recv_buf = [0u8; REPORT_SIZE + 1];
        recv_buf[0] = PREFIX_DEVICE_TO_HOST;
        let _ = self.device.get_input_report(&mut recv_buf);

        Ok(())
    }

    /// Send the start/stop edge stream command.
    pub fn edge_stream(&self, start: bool) -> Result<()> {
        self.edge_stream_with_params(if start { 1 } else { 0 }, start, false)
    }

    /// Send stereo camera init command.
    pub fn stereo_camera_init(&self) -> Result<()> {
        let _ = self.transaction(protocol::CMD_STEREO_CAMERA_INIT)?;
        Ok(())
    }

    /// Send stereo camera start command.
    pub fn stereo_camera_start(&self) -> Result<()> {
        let _ = self.transaction(protocol::CMD_STEREO_CAMERA_START)?;
        Ok(())
    }
}
