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
    constexpr int MAX_RECOVERY_ATTEMPTS = 3;
    constexpr uint8_t SLAM_ENDPOINT = 0x83;
}

namespace xv {

struct SlamContext {
    std::vector<slamCallback>* callbacks;
    std::atomic_bool* running;
    libusb_device_handle* handle;
    std::atomic<int>* frameCount;
    std::atomic<int> recoveryNeeded{0};  // 0 = ok, 1+ = recovery attempt number
};

Slam::Slam(Device* pDevice, libusb_context* libusbContext, libusb_device_handle* libusbDeviceHandle)
    : device(pDevice), handle(libusbDeviceHandle), context(libusbContext) {}

Slam::~Slam() {
    stop();
}

void Slam::start(mode slamMode) {
    bool isEdge = (slamMode == mode::Edge);
    // Official XSlamDriver uses uvcMode=0 for Edge, we match that
    device->configureDevice(isEdge, 0, !isEdge);

    // Official XSlamDriver sleeps 1s between configure and startEdgeStream
    std::this_thread::sleep_for(std::chrono::seconds(1));

    // edgeMode must match: 1 for Edge SLAM, 0 for Mixed/host-assisted
    // rotationEnabled=true needed for live rotation data (false freezes quaternion)
    device->startEdgeStream(isEdge ? 1 : 0, true, false);
    frameCount = 0;
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

int Slam::getFrameCount() const {
    return frameCount;
}

void Slam::registerSlamCallback(const std::function<void(Pose)>& callback) {
    callbacks.push_back(callback);
}

void Slam::slamHandler() {
    libusb_transfer* transfer = libusb_alloc_transfer(0);
    auto buffer = std::make_shared<std::array<uint8_t, 64>>();
    auto ctx = new SlamContext{&callbacks, &runThread, handle, &frameCount};

    libusb_fill_interrupt_transfer(transfer, handle, SLAM_ENDPOINT, buffer->data(), 63,
                                   &Slam::usbCallback, ctx, 5000);

    if (int result = libusb_submit_transfer(transfer); result != LIBUSB_SUCCESS) {
        std::cerr << "[XR50] Initial transfer error: " << libusb_strerror(result) << std::endl;
        runThread = false;
        libusb_free_transfer(transfer);
        delete ctx;
        return;
    }

    while (runThread) {
        struct timeval tv = {0, 1000};  // 1ms timeout
        libusb_handle_events_timeout(context, &tv);

        // Recovery runs here, OUTSIDE the async callback, where sync USB I/O is safe
        int attempt = ctx->recoveryNeeded.load();
        if (attempt > 0) {
            if (attempt > MAX_RECOVERY_ATTEMPTS) {
                std::cerr << "[XR50] Recovery failed after " << MAX_RECOVERY_ATTEMPTS
                          << " attempts, stopping." << std::endl;
                runThread = false;
                break;
            }

            int res = libusb_clear_halt(handle, SLAM_ENDPOINT);
            if (res == LIBUSB_ERROR_NO_DEVICE) {
                std::cerr << "[XR50] Device gone during recovery, stopping." << std::endl;
                runThread = false;
                break;
            }
            if (res != LIBUSB_SUCCESS && res != LIBUSB_ERROR_NOT_FOUND) {
                std::cerr << "[XR50] clear_halt: " << libusb_strerror(res) << std::endl;
            }

            std::this_thread::sleep_for(std::chrono::milliseconds(50 * attempt));

            res = libusb_submit_transfer(transfer);
            if (res == LIBUSB_SUCCESS) {
                std::cerr << "[XR50] Recovered on attempt " << attempt << std::endl;
                ctx->recoveryNeeded.store(0);
            } else if (res == LIBUSB_ERROR_NO_DEVICE) {
                std::cerr << "[XR50] Device gone during resubmit, stopping." << std::endl;
                runThread = false;
                break;
            } else {
                std::cerr << "[XR50] Resubmit failed: " << libusb_strerror(res) << std::endl;
                ctx->recoveryNeeded.fetch_add(1);
            }
        }
    }

    libusb_cancel_transfer(transfer);
    // Drain pending events so the cancel completes before we free
    struct timeval tv = {0, 100000};
    libusb_handle_events_timeout(context, &tv);
    libusb_free_transfer(transfer);
    delete ctx;
}

void Slam::usbCallback(libusb_transfer* transfer) {
    auto* ctx = static_cast<SlamContext*>(transfer->user_data);

    if (transfer->status != LIBUSB_TRANSFER_COMPLETED) {
        if (transfer->status == LIBUSB_TRANSFER_CANCELLED) return;
        if (!ctx->running->load()) return;

        const char* statusNames[] = {
            "COMPLETED", "ERROR", "TIMED_OUT", "CANCELLED", "STALL", "NO_DEVICE", "OVERFLOW"
        };
        int si = transfer->status;
        std::cerr << "[XR50] Transfer " << ((si >= 0 && si <= 6) ? statusNames[si] : "UNKNOWN")
                  << " after " << ctx->frameCount->load() << " frames" << std::endl;

        if (transfer->status == LIBUSB_TRANSFER_NO_DEVICE) {
            ctx->running->store(false);
            return;
        }

        // Signal the event loop to handle recovery (no sync USB I/O in callbacks!)
        ctx->recoveryNeeded.store(1);
        return;
    }

    // Resubmit FIRST for lowest latency (libusb_submit_transfer is safe in callbacks)
    if (ctx->running->load()) {
        int result = libusb_submit_transfer(transfer);
        if (result != LIBUSB_SUCCESS) {
            if (result == LIBUSB_ERROR_NO_DEVICE) {
                std::cerr << "[XR50] Device gone after " << ctx->frameCount->load() << " frames" << std::endl;
                ctx->running->store(false);
                return;
            }
            // Signal recovery — don't call libusb_clear_halt() here
            ctx->recoveryNeeded.store(1);
            return;
        }
    }

    // Parse pose data
    auto* buffer = transfer->buffer;
    int frame = ctx->frameCount->load();

    // Dump raw hex: first 3 frames of every session + every 200th
    if (frame < 3 || frame % 200 == 0) {
        std::cerr << "[XR50] Frame " << frame << " raw (" << transfer->actual_length << "B): ";
        for (int i = 0; i < transfer->actual_length && i < 63; i++) {
            char hex[4];
            snprintf(hex, sizeof(hex), "%02x", buffer[i]);
            std::cerr << hex;
            // Separators: header | timestamp | translation(12B) | quat(8B) | rest
            if (i == 2 || i == 6 || i == 18 || i == 26) std::cerr << " | ";
            else std::cerr << " ";
        }
        std::cerr << std::endl;
    }

    auto timestamp = *reinterpret_cast<uint32_t*>(&buffer[3]);
    auto* translationData = reinterpret_cast<int32_t*>(&buffer[7]);

    Vector3 position = {
        translationData[0] * FLOAT_SCALE,
        translationData[1] * FLOAT_SCALE,
        translationData[2] * FLOAT_SCALE
    };

    // Wire format is [w, x, y, z] (quatData[0]=w ≈ -1.0 for identity)
    // Note: SDK API uses [qx,qy,qz,qw] but device sends w first
    auto* quatData = reinterpret_cast<int16_t*>(&buffer[19]);
    Vector4 quaternion = {
        quatData[0] * FLOAT_SCALE,  // w
        quatData[1] * FLOAT_SCALE,  // x
        quatData[2] * FLOAT_SCALE,  // y
        quatData[3] * FLOAT_SCALE   // z
    };

    // Log quaternion and extra bytes for first few frames
    if (frame < 5) {
        std::cerr << "[XR50] Quat: w=" << quaternion[0] << " x=" << quaternion[1]
                  << " y=" << quaternion[2] << " z=" << quaternion[3] << std::endl;
        // Dump bytes 27-50 as int16 to look for more data
        std::cerr << "[XR50] Extra int16 @27: ";
        for (int i = 27; i + 1 < 63; i += 2) {
            auto val = *reinterpret_cast<int16_t*>(&buffer[i]);
            std::cerr << val << " ";
        }
        std::cerr << std::endl;
    }

    Pose pose{position, quaternion, timestamp};
    ctx->frameCount->fetch_add(1);

    for (const auto& callback : *ctx->callbacks) {
        callback(pose);
    }
}

} // namespace xv
