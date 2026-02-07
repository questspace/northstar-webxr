//
// Created by Mihir Patil on 8/6/23.
//

#include <iostream>
#include "xvisio.h"

namespace xv {
    XVisio::XVisio() {
        if (const int res = libusb_init(&usb_ctx); res != 0) {
            throw std::runtime_error("USBInitFailure");
        }

        libusb_device **list = nullptr;
        const ssize_t nDevices = libusb_get_device_list(usb_ctx, &list);
        for (int i = 0; i < nDevices; ++i) {
            libusb_device_descriptor desc = {};
            libusb_get_device_descriptor(list[i], &desc);
            if (desc.idVendor == 0x040e && desc.idProduct == 0xf408) {
                auto devPtr = std::make_shared<Device>(list[i], usb_ctx);
                this->devices.push_back(devPtr);
            }
        }
        libusb_free_device_list(list, 1);

        // Register hotplug — callback only queues the device pointer,
        // actual Device construction is deferred to pollNewDevices().
        libusb_hotplug_register_callback(
            usb_ctx, LIBUSB_HOTPLUG_EVENT_DEVICE_ARRIVED, LIBUSB_HOTPLUG_NO_FLAGS,
            0x040e, 0xf408, LIBUSB_HOTPLUG_MATCH_ANY,
            &XVisio::hotPlugCallback, this, &hotplugHandle);
    }

    XVisio::XVisio(uint32_t timeout) {

    }

    XVisio::~XVisio() {
        devices.clear();
        // Release any pending devices that were never processed
        {
            std::lock_guard<std::mutex> lock(pendingMutex);
            for (auto* dev : pendingDevices) {
                libusb_unref_device(dev);
            }
            pendingDevices.clear();
        }
        if (usb_ctx) {
            if (hotplugHandle) {
                libusb_hotplug_deregister_callback(usb_ctx, hotplugHandle);
            }
            libusb_exit(usb_ctx);
            usb_ctx = nullptr;
        }
    }

    int LIBUSB_CALL
    XVisio::hotPlugCallback(libusb_context*, libusb_device* device,
                            libusb_hotplug_event, void* user_data) {
        // This runs inside libusb_handle_events() — no synchronous USB I/O allowed!
        // Just ref the device and queue it for later construction.
        auto* self = static_cast<XVisio*>(user_data);
        libusb_ref_device(device);
        {
            std::lock_guard<std::mutex> lock(self->pendingMutex);
            self->pendingDevices.push_back(device);
        }
        std::cerr << "[XR50] Hotplug: device arrived (queued)" << std::endl;
        return 0;
    }

    void XVisio::pollNewDevices() {
        std::vector<libusb_device*> toProcess;
        {
            std::lock_guard<std::mutex> lock(pendingMutex);
            toProcess.swap(pendingDevices);
        }
        for (auto* dev : toProcess) {
            try {
                libusb_device_descriptor desc = {};
                libusb_get_device_descriptor(dev, &desc);
                if (desc.idVendor == 0x040e && desc.idProduct == 0xf408) {
                    auto devPtr = std::make_shared<Device>(dev, usb_ctx);
                    devices.push_back(devPtr);
                }
            } catch (const std::exception& e) {
                std::cerr << "[XR50] Hotplug device init: " << e.what() << std::endl;
            }
            libusb_unref_device(dev);
        }
    }

    const std::vector<std::shared_ptr<Device>> &XVisio::getDevices() {
        return devices;
    }

} // xv
