// xvsdk-bridge: the official XVisio SDK as a JSON-lines source for the Vibestar XR50 bridge.
//
// Prints one line per pose and per plane update on stdout:
//   {"x":..,"y":..,"z":..,"q":[qx,qy,qz,qw],"c":..,"tu":<device µs>,"t":<host s>}
//   {"type":"planes","planes":[{"id":"..","n":[nx,ny,nz],"d":..,"p":[[x,y,z],...]},...]}
//   {"type":"map","event":"saved|loaded|localized|error",...}
// Poses are the SDK's: IMU frame in the SDK world. The XR50 bridge converts them to the raw
// XR50 convention (see sdk_loop in xvisio-rs/examples/server.rs). The SDK itself also logs to
// stdout: only lines starting with '{' are data; our own logs go to stderr.
//
//   xvsdk-bridge [--mode mixed|edge] [--no-planes] [--map <file>] [--exit-on-stdin-eof]
//
// --map loads that map at start when it exists. With --exit-on-stdin-eof the helper reads
// commands from stdin ("save <file>", "load <file>") and stops when stdin closes.
//
// mixed: SLAM on the host from the fisheye cameras + IMU; enables stereo plane detection.
// edge:  SLAM on the device (same as xvisio-rs), for comparing frames.
#include <xv-sdk.h>
#include <xv-sdk-ex.h>

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <fstream>
#include <mutex>
#include <string>
#include <thread>

namespace {

std::mutex out;
std::atomic<bool> running{true};
// The SDK reads/writes the map asynchronously and reports in its done callback; the file
// stays open until then (one map operation at a time).
std::filebuf mapFile;

void emit(const std::string& line) {
    std::lock_guard<std::mutex> lock(out);
    std::fwrite(line.data(), 1, line.size(), stdout);
    std::fputc('\n', stdout);
    std::fflush(stdout);
}

std::string pose_json(const xv::Pose& p) {
    const auto& t = p.translation();
    const auto& q = p.quaternion();
    char buf[256];
    std::snprintf(buf, sizeof buf,
                  "{\"x\":%.5f,\"y\":%.5f,\"z\":%.5f,\"q\":[%.6f,%.6f,%.6f,%.6f],\"c\":%.3f,\"tu\":%lld,\"t\":%.6f}",
                  t[0], t[1], t[2], q[0], q[1], q[2], q[3], p.confidence(),
                  static_cast<long long>(p.edgeTimestampUs()), p.hostTimestamp());
    return buf;
}

std::string planes_json(const std::vector<xv::Plane>& planes) {
    std::string s = "{\"type\":\"planes\",\"planes\":[";
    char buf[128];
    for (size_t i = 0; i < planes.size(); ++i) {
        const auto& pl = planes[i];
        std::snprintf(buf, sizeof buf, "%s{\"id\":\"", i ? "," : "");
        s += buf;
        s += pl.id;
        std::snprintf(buf, sizeof buf, "\",\"n\":[%.4f,%.4f,%.4f],\"d\":%.4f,\"p\":[", pl.normal[0], pl.normal[1], pl.normal[2], pl.d);
        s += buf;
        for (size_t k = 0; k < pl.points.size(); ++k) {
            const auto& v = pl.points[k];
            std::snprintf(buf, sizeof buf, "%s[%.4f,%.4f,%.4f]", k ? "," : "", v[0], v[1], v[2]);
            s += buf;
        }
        s += "]}";
    }
    return s + "]}";
}

void map_event(const char* event, const char* fields) {
    char buf[200];
    std::snprintf(buf, sizeof buf, "{\"type\":\"map\",\"event\":\"%s\"%s%s}", event, *fields ? "," : "", fields);
    emit(buf);
}

/// How much of what SLAM sees comes from the reference map, at most once a second.
void on_localized(float share) {
    static auto last = std::chrono::steady_clock::time_point{};
    auto now = std::chrono::steady_clock::now();
    if (now - last < std::chrono::seconds(1)) return;
    last = now;
    char f[64];
    std::snprintf(f, sizeof f, "\"share\":%.2f", share);
    map_event("localized", f);
}

/// Saves the current map to `path` and keeps tracking against it (CSLAM): poses stay in the
/// saved map's frame, so a later load puts the world back where it was.
void save_map(xv::Slam& slam, const std::string& path) {
    if (mapFile.is_open() || !mapFile.open(path, std::ios::binary | std::ios::out | std::ios::trunc)) {
        map_event("error", "\"message\":\"cannot open the map file (or a map operation is running)\"");
        return;
    }
    slam.saveMapAndSwitchToCslam(mapFile, [](int status, int quality) {
        mapFile.close();
        char f[64];
        std::snprintf(f, sizeof f, "\"ok\":%s,\"quality\":%d", status == 2 ? "true" : "false", quality);
        map_event("saved", f);
    }, on_localized);
}

/// Loads a map saved earlier and relocalises into it.
void load_map(xv::Slam& slam, const std::string& path) {
    if (mapFile.is_open() || !mapFile.open(path, std::ios::binary | std::ios::in)) {
        map_event("error", "\"message\":\"no saved map (or a map operation is running)\"");
        return;
    }
    slam.loadMapAndSwitchToCslam(mapFile, [](int quality) {
        mapFile.close();
        char f[32];
        std::snprintf(f, sizeof f, "\"quality\":%d", quality);
        map_event("loaded", f);
    }, on_localized);
}

/// The fisheye calibration as JSON: per camera its pose in the IMU frame, the UCM and
/// polynomial parameter sets, and a grid of SDK-raytraced unit rays (every 16 px) for checking
/// our own camera model against the vendor's.
std::string calibration_json(const std::vector<xv::Calibration>& cams) {
    std::string s = "{\"cameras\":[";
    char buf[512];
    for (size_t c = 0; c < cams.size(); ++c) {
        const auto& cal = cams[c];
        const auto& R = cal.pose.rotation();
        const auto& t = cal.pose.translation();
        std::snprintf(buf, sizeof buf, "%s{\"rotation\":[%.9f,%.9f,%.9f,%.9f,%.9f,%.9f,%.9f,%.9f,%.9f],\"translation\":[%.9f,%.9f,%.9f],\"ucm\":[",
                      c ? "," : "", R[0], R[1], R[2], R[3], R[4], R[5], R[6], R[7], R[8], t[0], t[1], t[2]);
        s += buf;
        for (size_t i = 0; i < cal.ucm.size(); ++i) {
            const auto& m = cal.ucm[i];
            std::snprintf(buf, sizeof buf, "%s{\"w\":%d,\"h\":%d,\"fx\":%.9f,\"fy\":%.9f,\"u0\":%.9f,\"v0\":%.9f,\"xi\":%.9f}",
                          i ? "," : "", m.w, m.h, m.fx, m.fy, m.u0, m.v0, m.xi);
            s += buf;
        }
        s += "],\"pdcm\":[";
        for (size_t i = 0; i < cal.pdcm.size(); ++i) {
            const auto& m = cal.pdcm[i];
            std::snprintf(buf, sizeof buf, "%s{\"w\":%d,\"h\":%d,\"fx\":%.9f,\"fy\":%.9f,\"u0\":%.9f,\"v0\":%.9f,\"distor\":[%.9f,%.9f,%.9f,%.9f,%.9f]}",
                          i ? "," : "", m.w, m.h, m.fx, m.fy, m.u0, m.v0, m.distor[0], m.distor[1], m.distor[2], m.distor[3], m.distor[4]);
            s += buf;
        }
        s += "],\"models\":[";
        for (size_t i = 0; i < cal.camerasModel.size(); ++i) {
            const auto& m = cal.camerasModel[i];
            std::snprintf(buf, sizeof buf, "%s{\"w\":%d,\"h\":%d,\"rays\":[", i ? "," : "", m->width(), m->height());
            s += buf;
            bool first = true;
            for (int v = 0; v < m->height(); v += 16) {
                for (int u = 0; u < m->width(); u += 16) {
                    double p2[2] = {double(u), double(v)}, p3[3];
                    if (!m->raytrace(p2, p3)) continue;
                    std::snprintf(buf, sizeof buf, "%s[%d,%d,%.7f,%.7f,%.7f]", first ? "" : ",", u, v, p3[0], p3[1], p3[2]);
                    s += buf;
                    first = false;
                }
            }
            s += "]}";
        }
        s += "]}";
    }
    return s + "]}";
}

}  // namespace

int main(int argc, char* argv[]) try {
    std::string mode = "mixed";
    bool planes = true;
    bool imu = false;
    bool exit_on_eof = false;
    std::string map_path;
    bool dump_calibration = false;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a == "--mode" && i + 1 < argc) mode = argv[++i];
        if (a == "--no-planes") planes = false;
        if (a == "--imu") imu = true;  // also emit {"type":"imu","a":[..]} at ~20 Hz (frame calibration)
        if (a == "--exit-on-stdin-eof") exit_on_eof = true;  // read commands; stop when stdin closes
        if (a == "--map" && i + 1 < argc) map_path = argv[++i];  // load this map at start if it exists
        if (a == "--dump-calibration") dump_calibration = true;  // print the fisheye calibration as JSON and exit
    }
    std::signal(SIGINT, [](int) { running = false; });
    std::signal(SIGTERM, [](int) { running = false; });

    xv::setLogLevel(xv::LogLevel::warn);
    std::fprintf(stderr, "[xvsdk-bridge] XVSDK %s, mode %s, planes %s\n",
                 xv::version().toString().c_str(), mode.c_str(), planes ? "on" : "off");

    auto devices = xv::getDevices(10.);
    if (devices.empty()) {
        std::fprintf(stderr, "[xvsdk-bridge] no device found\n");
        return 2;
    }
    auto device = devices.begin()->second;
    if (dump_calibration) {
        if (!device->fisheyeCameras()) return 3;
        emit(calibration_json(device->fisheyeCameras()->calibration()));
        return 0;
    }
    auto on_pose = [](const xv::Pose& p) { emit(pose_json(p)); };

    if (mode == "edge") {
        auto ex = std::dynamic_pointer_cast<xv::DeviceEx>(device);
        if (!ex || !ex->slam2()) {
            std::fprintf(stderr, "[xvsdk-bridge] device has no edge SLAM\n");
            return 3;
        }
        ex->slam2()->registerCallback(on_pose);
        ex->slam2()->start(xv::Slam::Mode::Edge);
    } else {
        auto slam = device->slam();
        auto cams = device->fisheyeCameras();
        if (!slam || !cams) {
            std::fprintf(stderr, "[xvsdk-bridge] device has no SLAM or fisheye cameras\n");
            return 3;
        }
        // Host SLAM runs on the fisheye pair. Same order as the SDK's own sample (which works
        // on this device): IMU callback, camera callback, VGA resolution, cameras, then SLAM.
        auto step = [](const char* s) { std::fprintf(stderr, "[xvsdk-bridge] %s\n", s); };
        device->enableSync(false);
        if (imu && device->imuSensor()) {
            device->imuSensor()->registerCallback([](const xv::Imu& m) {
                static int k = 0;
                if (k++ % 50) return;
                char buf[160];
                std::snprintf(buf, sizeof buf, "{\"type\":\"imu\",\"a\":[%.4f,%.4f,%.4f],\"tu\":%lld}",
                              m.accel[0], m.accel[1], m.accel[2], static_cast<long long>(m.edgeTimestampUs));
                emit(buf);
            });
        }
        cams->registerCallback([](const xv::FisheyeImages&) {});
        if (auto ex = std::dynamic_pointer_cast<xv::FisheyeCamerasEx>(cams)) {
            ex->setResolutionMode(xv::FisheyeCamerasEx::ResolutionMode::MEDIUM);
        }
        step("starting cameras");
        cams->start();
        step("cameras started");
        slam->registerCallback(on_pose);
        if (planes) {
            slam->registerStereoPlanesCallback([](std::shared_ptr<const std::vector<xv::Plane>> p) {
                if (p) emit(planes_json(*p));
            });
        }
        step("starting SLAM");
        slam->start();
        if (!map_path.empty() && std::ifstream(map_path).good()) {
            step("loading the saved map");
            load_map(*slam, map_path);
        }
    }
    std::fprintf(stderr, "[xvsdk-bridge] streaming\n");

    // Commands from the parent bridge, one per line: "save <path>", "load <path>".
    // Closing stdin (the bridge exited) stops the helper.
    if (exit_on_eof) {
        std::thread([device, mode] {
            char line[1024];
            while (std::fgets(line, sizeof line, stdin)) {
                std::string cmd = line;
                cmd.erase(cmd.find_last_not_of("\r\n") + 1);
                auto space = cmd.find(' ');
                auto verb = cmd.substr(0, space);
                auto path = space == std::string::npos ? std::string() : cmd.substr(space + 1);
                if (mode != "mixed" || path.empty()) {
                    map_event("error", "\"message\":\"maps need --mode mixed and a path\"");
                } else if (verb == "save") {
                    save_map(*device->slam(), path);
                } else if (verb == "load") {
                    load_map(*device->slam(), path);
                }
            }
            running = false;
        }).detach();
    }
    while (running) std::this_thread::sleep_for(std::chrono::milliseconds(100));

    if (mode == "edge") std::dynamic_pointer_cast<xv::DeviceEx>(device)->slam2()->stop();
    else {
        device->slam()->stop();
        device->fisheyeCameras()->stop();
    }
    std::fprintf(stderr, "[xvsdk-bridge] stopped\n");
    return 0;
} catch (const std::exception& e) {
    std::fprintf(stderr, "[xvsdk-bridge] error: %s\n", e.what());
    return 1;
}
