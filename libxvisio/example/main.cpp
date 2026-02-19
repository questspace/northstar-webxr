/**
 * XVisio XR50 JSON Stream (with auto-reconnect)
 *
 * Outputs pose data as JSON lines to stdout for WebSocket bridging.
 * Automatically reconnects when the XR50 resets/disconnects.
 *
 * Usage: sudo ./xvisio_test | node server.js
 */

#include <iostream>
#include <iomanip>
#include <csignal>
#include <cstdio>
#include <cmath>
#include <atomic>
#include <chrono>
#include <thread>
#include "xvisio.h"

namespace {
    std::atomic<bool> running{true};
    constexpr int MAX_SESSION_RETRIES = 100;
    constexpr int EDGE_CRASH_THRESHOLD = 3;

    // Output throttle: one JSON line per interval
    constexpr int OUTPUT_INTERVAL_MS = 100;
    auto lastOutputTime = std::chrono::steady_clock::now();

    // Change detection for debugging
    double prevX = 0, prevY = 0, prevZ = 0;
    double prevW = 0, prevQx = 0, prevQy = 0, prevQz = 0;
    int changeCount = 0;
}

void onPose(const xv::Pose& pose) {
    auto now = std::chrono::steady_clock::now();
    auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(now - lastOutputTime).count();

    double w = pose.quaternion[0], x = pose.quaternion[1];
    double y = pose.quaternion[2], z = pose.quaternion[3];
    double px = pose.position[0], py = pose.position[1], pz = pose.position[2];

    // Detect changes in pose data
    bool posChanged = (px != prevX || py != prevY || pz != prevZ);
    bool rotChanged = (w != prevW || x != prevQx || y != prevQy || z != prevQz);

    if (posChanged || rotChanged) {
        changeCount++;
        std::cerr << "[XR50] POSE CHANGED (#" << changeCount << ")"
                  << " pos=" << posChanged << " rot=" << rotChanged
                  << " | pos(" << px << ", " << py << ", " << pz << ")"
                  << " | quat(" << w << ", " << x << ", " << y << ", " << z << ")"
                  << " t=" << pose.timestamp << std::endl;
        prevX = px; prevY = py; prevZ = pz;
        prevW = w; prevQx = x; prevQy = y; prevQz = z;
    }

    // Throttle JSON output to stdout
    if (elapsed < OUTPUT_INTERVAL_MS) return;
    lastOutputTime = now;

    double roll  = std::atan2(2.0 * (w * x + y * z), 1.0 - 2.0 * (x * x + y * y)) * 180.0 / M_PI;
    double pitch = std::asin(std::clamp(2.0 * (w * y - z * x), -1.0, 1.0)) * 180.0 / M_PI;
    double yaw   = std::atan2(2.0 * (w * z + x * y), 1.0 - 2.0 * (y * y + z * z)) * 180.0 / M_PI;

    std::cout << std::fixed << std::setprecision(4)
              << "{\"x\":" << px
              << ",\"y\":" << py
              << ",\"z\":" << pz
              << ",\"roll\":" << roll
              << ",\"pitch\":" << pitch
              << ",\"yaw\":" << yaw
              << ",\"t\":" << pose.timestamp << "}\n" << std::flush;
}

void printDeviceInfo(const std::shared_ptr<xv::Device>& dev) {
    std::cerr << "\n[XR50] UUID:     " << dev->getUUID() << std::endl;
    std::cerr << "[XR50] Firmware: " << dev->getVersion() << std::endl;
    std::cerr << "\n[XR50] Features:" << std::endl;
    std::cerr << "  Edge 6DOF:    " << (dev->getEdgeModeSupport() ? "YES" : "no") << std::endl;
    std::cerr << "  Mixed mode:   " << (dev->get_mixed_mode_support() ? "YES" : "no") << std::endl;
    std::cerr << "  Stereo:       " << (dev->getStereoSupport() ? "YES" : "no") << std::endl;
    std::cerr << "  RGB:          " << (dev->getRGBSupport() ? "YES" : "no") << std::endl;
    std::cerr << "  ToF:          " << (dev->getToFSupport() ? "YES" : "no") << std::endl;
    std::cerr << "  IA:           " << (dev->getIASupport() ? "YES" : "no") << std::endl;
    std::cerr << "  SGBM:         " << (dev->getSGBMSupport() ? "YES" : "no") << std::endl;
    std::cerr << "  Eye tracking: " << (dev->getEyeTrackingSupport() ? "YES" : "no") << std::endl;
    std::cerr << "  Face ID:      " << (dev->getFaceIDSupport() ? "YES" : "no") << std::endl;
}

/** Run one SLAM session. Returns number of frames received, -1 if no device found. */
int runSession(bool verbose, xv::Slam::mode slamMode) {
    xv::XVisio* xvisio = nullptr;
    std::shared_ptr<xv::Slam> slam;

    // Reset change detection per session
    prevX = prevY = prevZ = 0;
    prevW = prevQx = prevQy = prevQz = 0;
    changeCount = 0;

    try {
        xvisio = new xv::XVisio();
        auto& devices = xvisio->getDevices();
        if (devices.empty()) {
            delete xvisio;
            return -1;
        }

        auto& dev = devices[0];
        if (verbose) printDeviceInfo(dev);

        const char* modeName = (slamMode == xv::Slam::mode::Edge) ? "Edge" : "Mixed";
        std::cerr << "[XR50] Starting " << modeName << " SLAM..." << std::endl;

        slam = dev->getSlam();
        slam->registerSlamCallback(&onPose);
        slam->start(slamMode);

        while (running && slam->running()) {
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }

        int frames = slam->getFrameCount();
        slam->stop();
        slam.reset();
        delete xvisio;

        std::cerr << "[XR50] Session ended (" << frames << " frames, "
                  << changeCount << " pose changes)" << std::endl;
        return frames;

    } catch (const std::exception& e) {
        std::cerr << "[XR50] Session error: " << e.what() << std::endl;
        try { if (slam) slam->stop(); } catch (...) {}
        slam.reset();
        try { delete xvisio; } catch (...) {}
        return 0;
    } catch (...) {
        std::cerr << "[XR50] Unknown session error" << std::endl;
        try { if (slam) slam->stop(); } catch (...) {}
        slam.reset();
        try { delete xvisio; } catch (...) {}
        return 0;
    }
}

/** Interruptible sleep */
void sleepMs(int ms) {
    for (int i = 0; i < ms / 100 && running; i++) {
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
}

int main() {
    std::ios::sync_with_stdio(false);
    std::cout << std::unitbuf;
    std::setvbuf(stdout, nullptr, _IONBF, 0);

    std::signal(SIGINT, [](int) { running = false; });
    std::signal(SIGPIPE, SIG_IGN);

    int session = 0;
    int edgeCrashCount = 0;
    auto slamMode = xv::Slam::mode::Edge;

    while (running && session < MAX_SESSION_RETRIES) {
        bool verbose = (session == 0);
        int frames = runSession(verbose, slamMode);

        if (frames == -1) {
            if (session == 0) std::cerr << "[XR50] No device found, waiting..." << std::endl;
            sleepMs(2000);
            continue;
        }

        session++;
        if (!running) break;

        if (slamMode == xv::Slam::mode::Edge && frames < 100) {
            edgeCrashCount++;
            if (edgeCrashCount >= EDGE_CRASH_THRESHOLD) {
                std::cerr << "[XR50] Edge SLAM crashed " << edgeCrashCount
                          << " times, switching to Mixed mode" << std::endl;
                slamMode = xv::Slam::mode::Mixed;
            }
        } else {
            edgeCrashCount = 0;
        }

        int delayMs = (frames < 100) ? 6000 : 2000;
        std::cerr << "[XR50] Reconnecting in " << delayMs << "ms (session "
                  << session << "/" << MAX_SESSION_RETRIES << ")" << std::endl;
        sleepMs(delayMs);
    }

    if (session >= MAX_SESSION_RETRIES) {
        std::cerr << "[XR50] Max retries reached (" << MAX_SESSION_RETRIES << ")" << std::endl;
    }

    return 0;
}
