/**
 * @file slam.cpp
 * @brief SLAM tracking implementation
 */

#include "slam.h"
#include "device.h"
#include <array>
#include <iostream>

namespace {
    constexpr double FLOAT_SCALE = 6.103515625e-05;
}

namespace xv {

Slam::Slam(Device* pDevice, libusb_context* libusbContext, libusb_device_handle* libusbDeviceHandle)
    : device(pDevice), handle(libusbDeviceHandle), context(libusbContext) {}

Slam::~Slam() {
    stop();
}

void Slam::start(mode mode) {
    device->configureDevice(mode == mode::Edge, 1, mode != mode::Edge);
    device->startEdgeStream(1, false, false);
    runThread = true;
    slamThread = std::thread(&Slam::slamHandler, this);
}

void Slam::stop() {
    runThread = false;
    if (slamThread.joinable()) {
        slamThread.join();
    }
}

bool Slam::running() const {
    return runThread;
}

void Slam::registerSlamCallback(const std::function<void(Pose)>& callback) {
    callbacks.push_back(callback);
}

void Slam::slamHandler() {
    libusb_transfer* transfer = libusb_alloc_transfer(0);
    auto buffer = std::make_shared<std::array<uint8_t, 64>>();
    
    libusb_fill_interrupt_transfer(transfer, handle, 0x83, buffer->data(), 63,
                                   &Slam::usbCallback, &callbacks, 5000);
    
    if (int result = libusb_submit_transfer(transfer); result != LIBUSB_SUCCESS) {
        std::cerr << "USB transfer error: " << libusb_strerror(result) << std::endl;
        runThread = false;
        return;
    }
    
    while (runThread) {
        struct timeval tv = {0, 1000};  // 1ms timeout
        libusb_handle_events_timeout(context, &tv);
    }
    
    libusb_cancel_transfer(transfer);
}

void Slam::usbCallback(libusb_transfer* transfer) {
    if (transfer->status != LIBUSB_TRANSFER_COMPLETED) {
        return;
    }
    
    libusb_submit_transfer(transfer);
    
    auto* buffer = transfer->buffer;
    auto timestamp = *reinterpret_cast<uint32_t*>(&buffer[3]);
    auto* translationData = reinterpret_cast<int32_t*>(&buffer[7]);
    auto* rotationData = reinterpret_cast<int16_t*>(&buffer[19]);
    
    Vector3 position = {
        translationData[0] * FLOAT_SCALE,
        translationData[1] * FLOAT_SCALE,
        translationData[2] * FLOAT_SCALE
    };
    
    Matrix3 rotation = {{
        {rotationData[0] * FLOAT_SCALE, rotationData[1] * FLOAT_SCALE, rotationData[2] * FLOAT_SCALE},
        {rotationData[3] * FLOAT_SCALE, rotationData[4] * FLOAT_SCALE, rotationData[5] * FLOAT_SCALE},
        {rotationData[6] * FLOAT_SCALE, rotationData[7] * FLOAT_SCALE, rotationData[8] * FLOAT_SCALE}
    }};
    
    Pose pose{position, rotation, timestamp};
    
    auto* callbackList = static_cast<std::vector<slamCallback>*>(transfer->user_data);
    for (const auto& callback : *callbackList) {
        callback(pose);
    }
}

} // namespace xv
