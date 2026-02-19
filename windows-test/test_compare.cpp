/**
 * @file test_compare.cpp
 * @brief Side-by-side comparison: Official XSlam SDK vs raw libusb HID
 *
 * Runs both the official SDK (xslam_sdk.dll via runtime loading) and
 * raw libusb HID simultaneously, comparing pose data.
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <thread>
#include <atomic>
#include <array>
#include <fstream>
#include <cmath>
#include <vector>

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <libusb.h>
#include "xslam_sdk.h"
#include "xslam_drivers.h"

/* -------------------------------------------------------------------------- */
/*  Signal handling                                                            */
/* -------------------------------------------------------------------------- */

static std::atomic<bool> g_running{true};

static BOOL WINAPI consoleHandler(DWORD signal) {
    if (signal == CTRL_C_EVENT || signal == CTRL_BREAK_EVENT) {
        g_running = false;
        return TRUE;
    }
    return FALSE;
}

/* -------------------------------------------------------------------------- */
/*  Runtime loading for XSlam SDK                                              */
/* -------------------------------------------------------------------------- */

static HMODULE g_hModule = nullptr;

static pfn_init_algorithm_and_loader  fp_init = nullptr;
static pfn_xslam_free                fp_free = nullptr;
static pfn_xslam_wait_for_camera     fp_wait = nullptr;
static pfn_xslam_start_camera        fp_start_cam = nullptr;
static pfn_xslam_start_edge_vo       fp_start_edge = nullptr;
static pfn_xslam_stop                fp_stop = nullptr;
static pfn_xslam_get_pose            fp_get_pose = nullptr;
static pfn_xslam_get_pose_quaternion fp_get_pose_quat = nullptr;
static pfn_xslam_set_debug_level     fp_debug = nullptr;
static pfn_xslam_hid_write_read      fp_hid_wr = nullptr;

template<typename T>
static T loadFn(HMODULE h, const char* mn) {
    return reinterpret_cast<T>(GetProcAddress(h, mn));
}

static bool loadSDK() {
    g_hModule = LoadLibraryA("xslam_sdk.dll");
    if (!g_hModule)
        g_hModule = LoadLibraryA("../project-esky-unity/Assets/Plugins/x64/xslam_sdk.dll");
    if (!g_hModule) return false;

    fp_init       = loadFn<pfn_init_algorithm_and_loader>(g_hModule, XSLAM_MN_INIT);
    fp_free       = loadFn<pfn_xslam_free>(g_hModule, XSLAM_MN_FREE);
    fp_wait       = loadFn<pfn_xslam_wait_for_camera>(g_hModule, XSLAM_MN_WAIT_FOR_CAMERA);
    fp_start_cam  = loadFn<pfn_xslam_start_camera>(g_hModule, XSLAM_MN_START_CAMERA);
    fp_start_edge = loadFn<pfn_xslam_start_edge_vo>(g_hModule, XSLAM_MN_START_EDGE_VO);
    fp_stop       = loadFn<pfn_xslam_stop>(g_hModule, XSLAM_MN_STOP);
    fp_get_pose   = loadFn<pfn_xslam_get_pose>(g_hModule, XSLAM_MN_GET_POSE);
    fp_get_pose_quat = loadFn<pfn_xslam_get_pose_quaternion>(g_hModule, XSLAM_MN_GET_POSE_QUAT);
    fp_debug      = loadFn<pfn_xslam_set_debug_level>(g_hModule, XSLAM_MN_SET_DEBUG_LEVEL);
    fp_hid_wr     = loadFn<pfn_xslam_hid_write_read>(g_hModule, XSLAM_MN_HID_WRITE_READ);

    return (fp_init != nullptr);
}

/* -------------------------------------------------------------------------- */
/*  Data structures                                                            */
/* -------------------------------------------------------------------------- */

struct PoseSample {
    double wallClockMs;
    double x, y, z;
    double r[9];
    uint32_t edgeTs;
    bool valid;
};

/* -------------------------------------------------------------------------- */
/*  Raw HID transaction                                                        */
/* -------------------------------------------------------------------------- */

static bool rawHidTransaction(libusb_device_handle* handle,
                               const unsigned char* cmd, int cmdLen,
                               unsigned char* resp, int respLen) {
    constexpr uint8_t reqType = LIBUSB_REQUEST_TYPE_CLASS | LIBUSB_RECIPIENT_INTERFACE;

    std::array<unsigned char, 63> sendBuf = {};
    sendBuf[0] = 0x02;
    memcpy(sendBuf.data() + 1, cmd, cmdLen < 62 ? cmdLen : 62);

    int res = libusb_control_transfer(handle, LIBUSB_ENDPOINT_OUT | reqType,
        XSLAM_HID_SET_REPORT, XSLAM_HID_REPORT_TYPE_OUTPUT, XSLAM_HID_INTERFACE,
        sendBuf.data(), 63, 1000);
    if (res < 0) return false;

    std::array<unsigned char, 63> recvBuf = {};
    res = libusb_control_transfer(handle, LIBUSB_ENDPOINT_IN | reqType,
        XSLAM_HID_GET_REPORT, XSLAM_HID_REPORT_TYPE_INPUT, XSLAM_HID_INTERFACE,
        recvBuf.data(), 63, 1000);
    if (res < 0) return false;
    if (recvBuf[0] != 0x01) return false;

    int dataStart = 1 + cmdLen;
    int copyLen = (63 - dataStart) < respLen ? (63 - dataStart) : respLen;
    if (copyLen > 0) memcpy(resp, recvBuf.data() + dataStart, copyLen);
    return true;
}

/* -------------------------------------------------------------------------- */
/*  Main                                                                       */
/* -------------------------------------------------------------------------- */

int main(int argc, char* argv[]) {
    int durationSec = 10;
    std::string reportPath;

    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--duration") == 0 && i + 1 < argc)
            durationSec = atoi(argv[++i]);
        else if (strcmp(argv[i], "--report") == 0 && i + 1 < argc)
            reportPath = argv[++i];
    }

    SetConsoleCtrlHandler(consoleHandler, TRUE);
    printf("=== XSlam vs Raw HID Comparison Test ===\n\n");

    // ================================================================
    // Part A: Raw libusb
    // ================================================================
    printf("=== Part A: Raw libusb HID ===\n\n");

    libusb_context* ctx = nullptr;
    libusb_device_handle* rawHandle = nullptr;
    bool rawAvailable = false;

    if (libusb_init(&ctx) == 0) {
        libusb_device** devList = nullptr;
        ssize_t nDevs = libusb_get_device_list(ctx, &devList);
        for (ssize_t i = 0; i < nDevs; ++i) {
            libusb_device_descriptor desc = {};
            libusb_get_device_descriptor(devList[i], &desc);
            if (desc.idVendor == XSLAM_VID && desc.idProduct == XSLAM_PID) {
                if (libusb_open(devList[i], &rawHandle) == 0) {
                    printf("[RAW] Device opened\n");
                    break;
                }
            }
        }
        libusb_free_device_list(devList, 1);

        if (rawHandle) {
            if (libusb_claim_interface(rawHandle, XSLAM_HID_INTERFACE) == 0) {
                rawAvailable = true;
                printf("[RAW] Interface %d claimed\n", XSLAM_HID_INTERFACE);

                unsigned char cmd[] = {0xfd, 0x66, 0x00, 0x02};
                unsigned char resp[58] = {};
                if (rawHidTransaction(rawHandle, cmd, sizeof(cmd), resp, sizeof(resp)))
                    printf("[RAW] UUID: %s\n", (char*)resp);

                unsigned char vcmd[] = {0x1c, 0x99};
                unsigned char vresp[60] = {};
                if (rawHidTransaction(rawHandle, vcmd, sizeof(vcmd), vresp, sizeof(vresp)))
                    printf("[RAW] Version: %s\n", (char*)vresp);

                unsigned char fcmd[] = {0xde, 0x62, 0x01};
                unsigned char fresp[59] = {};
                if (rawHidTransaction(rawHandle, fcmd, sizeof(fcmd), fresp, sizeof(fresp))) {
                    uint32_t feat = fresp[0] | (fresp[1] << 8) | (fresp[2] << 16) | (fresp[3] << 24);
                    printf("[RAW] Features: 0x%08x\n", feat);
                }
            }
        } else {
            printf("[RAW] XR50 not found\n");
        }
    }

    // ================================================================
    // Part B: Official SDK
    // ================================================================
    printf("\n=== Part B: Official XSlam SDK ===\n\n");

    bool sdkAvailable = false;

    if (loadSDK()) {
        printf("[SDK] DLL loaded\n");
        if (fp_debug) fp_debug(1);

        xslam_status initResult = fp_init ? fp_init() : XSLAM_ERROR;
        printf("[SDK] init_algorithm_and_loader() = %d\n", initResult);

        if (initResult == XSLAM_OK) {
            sdkAvailable = true;
            if (fp_wait) fp_wait();
            if (fp_start_cam) fp_start_cam();
            if (fp_start_edge) fp_start_edge();

            // SDK HID test
            if (fp_hid_wr) {
                unsigned char cmd[] = {0x02, 0xfd, 0x66, 0x00, 0x02};
                unsigned char resp[64] = {};
                bool ok = fp_hid_wr(cmd, sizeof(cmd), resp, sizeof(resp));
                if (ok) {
                    printf("[SDK] UUID (HID): ");
                    for (int i = 0; i < 32 && resp[i]; ++i) putchar(resp[i]);
                    printf("\n");
                }
            }
        }
    } else {
        printf("[SDK] Could not load xslam_sdk.dll\n");
    }

    // ================================================================
    // Part C: Comparison
    // ================================================================
    if (!rawAvailable && !sdkAvailable) {
        fprintf(stderr, "\nERROR: Neither raw HID nor SDK available.\n");
        if (rawHandle) { libusb_release_interface(rawHandle, XSLAM_HID_INTERFACE); libusb_close(rawHandle); }
        if (ctx) libusb_exit(ctx);
        return 1;
    }

    printf("\n=== Part C: Comparison (%d seconds) ===\n\n", durationSec);

    // Start raw streaming
    if (rawAvailable) {
        unsigned char cfgCmd[] = {0x19, 0x95, 0x01, 0x01, 0x00};
        unsigned char cfgResp[57] = {};
        rawHidTransaction(rawHandle, cfgCmd, sizeof(cfgCmd), cfgResp, sizeof(cfgResp));

        unsigned char startCmd[] = {0xa2, 0x33, 0x01, 0x00, 0x00};
        unsigned char startResp[57] = {};
        rawHidTransaction(rawHandle, startCmd, sizeof(startCmd), startResp, sizeof(startResp));
        printf("[RAW] Edge stream started\n");
    }

    std::vector<PoseSample> rawSamples, sdkSamples;
    auto startTime = std::chrono::steady_clock::now();

    while (g_running) {
        auto elapsed = std::chrono::steady_clock::now() - startTime;
        double elapsedMs = std::chrono::duration<double, std::milli>(elapsed).count();
        if (elapsedMs >= durationSec * 1000.0) break;

        // Raw HID packet
        if (rawAvailable) {
            unsigned char buffer[64] = {};
            int transferred = 0;
            int res = libusb_interrupt_transfer(rawHandle, XSLAM_SLAM_ENDPOINT,
                                                 buffer, 64, &transferred, 100);
            if (res == LIBUSB_SUCCESS && transferred > 0) {
                PoseSample s = {};
                s.wallClockMs = elapsedMs;
                s.edgeTs = *reinterpret_cast<uint32_t*>(&buffer[XSLAM_PKT_TIMESTAMP_OFFSET]);
                auto* tInts = reinterpret_cast<int32_t*>(&buffer[XSLAM_PKT_TRANSLATION_OFFSET]);
                s.x = tInts[0] * XSLAM_FLOAT_SCALE;
                s.y = tInts[1] * XSLAM_FLOAT_SCALE;
                s.z = tInts[2] * XSLAM_FLOAT_SCALE;
                auto* rShorts = reinterpret_cast<int16_t*>(&buffer[XSLAM_PKT_ROTATION_OFFSET]);
                for (int i = 0; i < 9; ++i) s.r[i] = rShorts[i] * XSLAM_FLOAT_SCALE;
                s.valid = true;
                rawSamples.push_back(s);
            }
        }

        // SDK pose
        if (sdkAvailable && fp_get_pose) {
            xslam_pose pose = {};
            xslam_status r = fp_get_pose(&pose, 0.0);
            if (r == XSLAM_OK) {
                PoseSample s = {};
                s.wallClockMs = elapsedMs;
                s.x = pose.translation[0];
                s.y = pose.translation[1];
                s.z = pose.translation[2];
                for (int i = 0; i < 9; ++i) s.r[i] = pose.rotation[i];
                s.edgeTs = (uint32_t)pose.edge_timestamp_us;
                s.valid = true;
                sdkSamples.push_back(s);
            }
        }

        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    // ================================================================
    // Part D: Report
    // ================================================================
    printf("\n=== Part D: Results ===\n\n");

    auto totalMs = std::chrono::duration<double, std::milli>(
        std::chrono::steady_clock::now() - startTime).count();

    printf("Duration: %.1f ms\n\n", totalMs);

    printf("RAW HID:\n");
    printf("  Samples: %zu\n", rawSamples.size());
    if (!rawSamples.empty()) {
        printf("  Rate: %.1f Hz\n", rawSamples.size() * 1000.0 / totalMs);
        printf("  First: [%.4f, %.4f, %.4f] ts=%u\n",
               rawSamples[0].x, rawSamples[0].y, rawSamples[0].z, rawSamples[0].edgeTs);
        printf("  Last:  [%.4f, %.4f, %.4f] ts=%u\n",
               rawSamples.back().x, rawSamples.back().y, rawSamples.back().z,
               rawSamples.back().edgeTs);
    }

    printf("\nSDK:\n");
    printf("  Samples: %zu\n", sdkSamples.size());
    if (!sdkSamples.empty()) {
        printf("  Rate: %.1f Hz\n", sdkSamples.size() * 1000.0 / totalMs);
        printf("  First: [%.4f, %.4f, %.4f] ts=%u\n",
               sdkSamples[0].x, sdkSamples[0].y, sdkSamples[0].z, sdkSamples[0].edgeTs);
        printf("  Last:  [%.4f, %.4f, %.4f] ts=%u\n",
               sdkSamples.back().x, sdkSamples.back().y, sdkSamples.back().z,
               sdkSamples.back().edgeTs);
    }

    // Timestamp matching
    if (!rawSamples.empty() && !sdkSamples.empty()) {
        printf("\nComparison:\n");
        int matches = 0;
        double totalPosDiff = 0;
        for (size_t i = 0; i < sdkSamples.size() && i < 100; ++i) {
            for (size_t j = 0; j < rawSamples.size(); ++j) {
                if (rawSamples[j].edgeTs == sdkSamples[i].edgeTs) {
                    double dx = rawSamples[j].x - sdkSamples[i].x;
                    double dy = rawSamples[j].y - sdkSamples[i].y;
                    double dz = rawSamples[j].z - sdkSamples[i].z;
                    double dist = sqrt(dx*dx + dy*dy + dz*dz);
                    totalPosDiff += dist;
                    matches++;
                    if (matches <= 5) {
                        printf("  Match #%d (ts=%u): RAW=[%.4f,%.4f,%.4f] SDK=[%.4f,%.4f,%.4f] diff=%.6f\n",
                               matches, rawSamples[j].edgeTs,
                               rawSamples[j].x, rawSamples[j].y, rawSamples[j].z,
                               sdkSamples[i].x, sdkSamples[i].y, sdkSamples[i].z, dist);
                    }
                    break;
                }
            }
        }
        printf("  Matched timestamps: %d\n", matches);
        if (matches > 0)
            printf("  Average position diff: %.6f m\n", totalPosDiff / matches);
    }

    // Write report
    if (!reportPath.empty()) {
        std::ofstream report(reportPath);
        report << "=== XSlam vs Raw HID Comparison Report ===\n\n";
        report << "Duration: " << totalMs << " ms\n";
        report << "RAW samples: " << rawSamples.size() << "\n";
        report << "SDK samples: " << sdkSamples.size() << "\n\n";
        report << "--- RAW HID (first 100) ---\nwall_ms,edge_ts,x,y,z\n";
        for (size_t i = 0; i < rawSamples.size() && i < 100; ++i)
            report << rawSamples[i].wallClockMs << "," << rawSamples[i].edgeTs << ","
                   << rawSamples[i].x << "," << rawSamples[i].y << "," << rawSamples[i].z << "\n";
        report << "\n--- SDK (first 100) ---\nwall_ms,edge_ts,x,y,z\n";
        for (size_t i = 0; i < sdkSamples.size() && i < 100; ++i)
            report << sdkSamples[i].wallClockMs << "," << sdkSamples[i].edgeTs << ","
                   << sdkSamples[i].x << "," << sdkSamples[i].y << "," << sdkSamples[i].z << "\n";
        printf("\nReport written to: %s\n", reportPath.c_str());
    }

    // Cleanup
    if (sdkAvailable) {
        if (fp_stop) fp_stop();
        if (fp_free) fp_free();
    }
    if (rawAvailable) libusb_release_interface(rawHandle, XSLAM_HID_INTERFACE);
    if (rawHandle) libusb_close(rawHandle);
    if (ctx) libusb_exit(ctx);
    if (g_hModule) FreeLibrary(g_hModule);

    printf("\nDone.\n");
    return 0;
}
