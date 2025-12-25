/**
 * XVisio XR50 JSON Stream
 * Outputs pose data as JSON lines to stdout for WebSocket bridging.
 * Usage: sudo ./xvisio_test | node server.js
 */

#include <iostream>
#include <iomanip>
#include <csignal>
#include <cstdio>
#include <cmath>
#include <atomic>
#include "xvisio.h"

namespace {
    std::atomic<bool> running{true};
}

void onPose(const xv::Pose& pose) {
    double w = pose.quaternion[0], x = pose.quaternion[1];
    double y = pose.quaternion[2], z = pose.quaternion[3];
    
    double roll  = std::atan2(2.0 * (w * x + y * z), 1.0 - 2.0 * (x * x + y * y)) * 180.0 / M_PI;
    double pitch = std::asin(std::clamp(2.0 * (w * y - z * x), -1.0, 1.0)) * 180.0 / M_PI;
    double yaw   = std::atan2(2.0 * (w * z + x * y), 1.0 - 2.0 * (y * y + z * z)) * 180.0 / M_PI;
    
    std::cout << std::fixed << std::setprecision(4)
              << "{\"x\":" << pose.position[0]
              << ",\"y\":" << pose.position[1]
              << ",\"z\":" << pose.position[2]
              << ",\"roll\":" << roll
              << ",\"pitch\":" << pitch
              << ",\"yaw\":" << yaw
              << ",\"t\":" << pose.timestamp << "}\n" << std::flush;
}

int main() {
    // Force unbuffered stdout for pipe compatibility
    std::ios::sync_with_stdio(false);
    std::cout << std::unitbuf;
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    
    std::signal(SIGINT, [](int) { running = false; });
    std::signal(SIGPIPE, SIG_IGN);
    
    try {
        xv::XVisio xvisio;
        auto& devices = xvisio.getDevices();
        if (devices.empty()) {
            std::cerr << "No XVisio device found\n";
            return 1;
        }
        
        auto slam = devices[0]->getSlam();
        slam->registerSlamCallback(&onPose);
        slam->start(xv::Slam::mode::Edge);
        
        while (running && slam->running()) {
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
        
        slam->stop();
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << "\n";
        return 1;
    }
    return 0;
}
