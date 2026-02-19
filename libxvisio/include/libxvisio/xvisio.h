//
// Created by Mihir Patil on 8/6/23.
//

#ifndef LIBXVISIO_XVISIO_H
#define LIBXVISIO_XVISIO_H

#include <vector>
#include <memory>
#include <mutex>
#include <libusb.h>
#include "device.h"
#include "slam.h"

namespace xv {
    class XVisio {
    public:
        XVisio();

        explicit XVisio(uint32_t timeout);

        ~XVisio();

        const std::vector<std::shared_ptr<Device>>& getDevices();

        /// Process devices discovered via hotplug. Call from main thread periodically.
        void pollNewDevices();

    private:
        libusb_context* usb_ctx = nullptr;
        std::vector<std::shared_ptr<Device>> devices;

        // Hotplug queues raw device pointers; Device construction happens outside the callback
        std::mutex pendingMutex;
        std::vector<libusb_device*> pendingDevices;
        libusb_hotplug_callback_handle hotplugHandle = 0;

        static int LIBUSB_CALL
        hotPlugCallback(libusb_context* ctx, libusb_device* device,
                        libusb_hotplug_event event, void* user_data);
    };
} // xv

#endif //LIBXVISIO_XVISIO_H
