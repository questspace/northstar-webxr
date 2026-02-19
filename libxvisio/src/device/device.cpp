/**
 * @file device.cpp
 * @brief XVisio device implementation
 */

#include "device.h"
#include "hid.h"
#include "slam.h"
#include <array>
#include <stdexcept>

namespace xv {

Device::Device(libusb_device* libusbDevice, libusb_context* context) 
    : libusbDevice(libusbDevice), libusbContext(context) {
    
    int res = libusb_open(libusbDevice, &handle);
    if (res != 0) {
        switch (res) {
            case LIBUSB_ERROR_ACCESS:
                throw std::runtime_error("Access denied. Run with sudo on macOS.");
            case LIBUSB_ERROR_NO_DEVICE:
                throw std::runtime_error("Device disconnected.");
            default:
                throw std::runtime_error(std::string("USB error: ") + libusb_strerror(res));
        }
    }
    
    hid = std::make_unique<HID>(handle);
    
    // Detach kernel driver if active
    if (libusb_kernel_driver_active(handle, 3) == 1) {
        libusb_detach_kernel_driver(handle, 3);
    }
    
    res = libusb_claim_interface(handle, 3);
    if (res != 0) {
        throw std::runtime_error(std::string("Cannot claim interface: ") + libusb_strerror(res));
    }
    
    // Read device UUID
    std::array<uint8_t, 4> uuidCmd = {0xfd, 0x66, 0x00, 0x02};
    std::array<uint8_t, 58> uuidResult = {0};
    if (hid->executeTransaction(uuidCmd, uuidResult)) {
        uuid = std::string(reinterpret_cast<const char*>(uuidResult.data()));
    } else {
        throw std::runtime_error("Failed to read device UUID");
    }
    
    // Read firmware version
    std::array<uint8_t, 2> versionCmd = {0x1c, 0x99};
    std::array<uint8_t, 60> versionResult = {0};
    if (hid->executeTransaction(versionCmd, versionResult)) {
        version = std::string(reinterpret_cast<const char*>(versionResult.data()));
    } else {
        throw std::runtime_error("Failed to read firmware version");
    }
    
    // Read device features
    std::array<uint8_t, 3> featuresCmd = {0xde, 0x62, 0x01};
    std::array<uint8_t, 59> featuresResult = {0};
    if (hid->executeTransaction(featuresCmd, featuresResult)) {
        featuresBitmap = featuresResult[0] | (featuresResult[1] << 8) | 
                         (featuresResult[2] << 16) | (featuresResult[3] << 24);
    } else {
        throw std::runtime_error("Failed to read device features");
    }
    
    slam = std::make_shared<Slam>(this, context, handle);
}

Device::~Device() {
    // Safe cleanup — device may already be disconnected
    if (handle) {
        libusb_release_interface(handle, 3); // Ignoring errors is fine here
        libusb_close(handle);
        handle = nullptr;
    }
}

std::string& Device::getUUID() { return uuid; }
std::string& Device::getVersion() { return version; }

bool Device::getEdgeModeSupport() const { return featuresBitmap & (1 << 0); }
bool Device::get_mixed_mode_support() const { return featuresBitmap & (1 << 1); }
bool Device::getStereoSupport() const { return featuresBitmap & (1 << 2); }
bool Device::getRGBSupport() const { return featuresBitmap & (1 << 3); }
bool Device::getToFSupport() const { return featuresBitmap & (1 << 4); }
bool Device::getIASupport() const { return featuresBitmap & (1 << 5); }
bool Device::getSGBMSupport() const { return featuresBitmap & (1 << 6); }
bool Device::getEyeTrackingSupport() const { return featuresBitmap & (1 << 10); }
bool Device::getFaceIDSupport() const { return featuresBitmap & (1 << 12); }

void Device::configureDevice(bool edge6dof, uint8_t uvcMode, bool embeddedAlgo) const {
    std::array<uint8_t, 5> cmd = {0x19, 0x95, edge6dof, uvcMode, embeddedAlgo};
    std::array<uint8_t, 57> result = {0};
    hid->executeTransaction(cmd, result);
}

void Device::startEdgeStream(uint8_t edgeMode, bool rotationEnabled, bool flipped) const {
    std::array<uint8_t, 5> cmd = {0xa2, 0x33, edgeMode, rotationEnabled, flipped};
    std::array<uint8_t, 57> result = {0};
    hid->executeTransaction(cmd, result);
}

std::shared_ptr<Slam> Device::getSlam() const { return slam; }

} // namespace xv
