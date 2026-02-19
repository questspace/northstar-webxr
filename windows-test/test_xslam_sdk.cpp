/**
 * @file test_xslam_sdk.cpp
 * @brief Test harness for the official XSlam SDK (xslam_sdk.dll)
 *
 * Uses runtime loading (LoadLibrary + GetProcAddress) with the exact
 * C++ mangled export names from xslam_sdk.dll.
 *
 * Usage:
 *   test_xslam_sdk.exe [--csv output.csv] [--duration <seconds>]
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <thread>
#include <string>
#include <fstream>
#include <atomic>

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

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
/*  Runtime loading                                                            */
/* -------------------------------------------------------------------------- */

static HMODULE g_hModule = nullptr;

// Function pointers
static pfn_init_algorithm_and_loader     fp_init = nullptr;
static pfn_xslam_free                   fp_free = nullptr;
static pfn_xslam_camera_is_detected     fp_camera_is_detected = nullptr;
static pfn_xslam_wait_for_camera        fp_wait_for_camera = nullptr;
static pfn_xslam_start_camera           fp_start_camera = nullptr;
static pfn_xslam_start_vo               fp_start_vo = nullptr;
static pfn_xslam_start_edge_vo          fp_start_edge_vo = nullptr;
static pfn_xslam_stop                   fp_stop = nullptr;
static pfn_xslam_get_pose               fp_get_pose = nullptr;
static pfn_xslam_get_pose_quaternion    fp_get_pose_quat = nullptr;
static pfn_xslam_get_nb_poses           fp_get_nb_poses = nullptr;
static pfn_xslam_set_debug_level        fp_set_debug_level = nullptr;
static pfn_xslam_set_coordinate_system  fp_set_coord_sys = nullptr;
static pfn_xslam_json_config            fp_json_config = nullptr;
static pfn_xslam_reset                  fp_reset = nullptr;
static pfn_xslam_reset_slam             fp_reset_slam = nullptr;
static pfn_xslam_hid_write_read         fp_hid_write_read = nullptr;
static pfn_xslam_hid_write_read_timeout fp_hid_write_read_timeout = nullptr;
static pfn_xslam_hid_write              fp_hid_write = nullptr;
static pfn_xslam_hid_read               fp_hid_read = nullptr;
static pfn_xslam_hid_get_report         fp_hid_get_report = nullptr;
static pfn_xslam_host_time_now          fp_host_time_now = nullptr;
static pfn_xslam_disp_version           fp_disp_version = nullptr;
static pfn_xslam_has_rgb                fp_has_rgb = nullptr;
static pfn_xslam_has_tof                fp_has_tof = nullptr;
static pfn_xslam_6dof_callback          fp_6dof_cb = nullptr;
static pfn_xslam_edge_6dof_callback     fp_edge_6dof_cb = nullptr;
static pfn_xslam_edge_6dof_quaternion_callback fp_edge_6dof_quat_cb = nullptr;
static pfn_xslam_clear_callbacks        fp_clear_callbacks = nullptr;

template<typename T>
static T loadFunc(HMODULE h, const char* mangledName, const char* readableName) {
    auto p = reinterpret_cast<T>(GetProcAddress(h, mangledName));
    if (p) {
        printf("  [OK]   %s\n", readableName);
    } else {
        printf("  [MISS] %s\n", readableName);
    }
    return p;
}

static bool loadXSlamSDK() {
    printf("Loading xslam_sdk.dll...\n");

    // Try current directory first, then relative path to project-esky-unity
    g_hModule = LoadLibraryA("xslam_sdk.dll");
    if (!g_hModule)
        g_hModule = LoadLibraryA("../project-esky-unity/Assets/Plugins/x64/xslam_sdk.dll");
    if (!g_hModule) {
        fprintf(stderr, "ERROR: Could not load xslam_sdk.dll (error %lu)\n", GetLastError());
        return false;
    }

    printf("  DLL loaded at %p\n\n", (void*)g_hModule);
    printf("Resolving exports (C++ mangled names):\n");

    fp_init               = loadFunc<pfn_init_algorithm_and_loader>(g_hModule, XSLAM_MN_INIT, "init_algorithm_and_loader");
    fp_free               = loadFunc<pfn_xslam_free>(g_hModule, XSLAM_MN_FREE, "xslam_free");
    fp_camera_is_detected = loadFunc<pfn_xslam_camera_is_detected>(g_hModule, XSLAM_MN_CAMERA_IS_DETECTED, "xslam_camera_is_detected");
    fp_wait_for_camera    = loadFunc<pfn_xslam_wait_for_camera>(g_hModule, XSLAM_MN_WAIT_FOR_CAMERA, "xslam_wait_for_camera");
    fp_start_camera       = loadFunc<pfn_xslam_start_camera>(g_hModule, XSLAM_MN_START_CAMERA, "xslam_start_camera");
    fp_start_vo           = loadFunc<pfn_xslam_start_vo>(g_hModule, XSLAM_MN_START_VO, "xslam_start_vo");
    fp_start_edge_vo      = loadFunc<pfn_xslam_start_edge_vo>(g_hModule, XSLAM_MN_START_EDGE_VO, "xslam_start_edge_vo");
    fp_stop               = loadFunc<pfn_xslam_stop>(g_hModule, XSLAM_MN_STOP, "xslam_stop");
    fp_get_pose           = loadFunc<pfn_xslam_get_pose>(g_hModule, XSLAM_MN_GET_POSE, "xslam_get_pose");
    fp_get_pose_quat      = loadFunc<pfn_xslam_get_pose_quaternion>(g_hModule, XSLAM_MN_GET_POSE_QUAT, "xslam_get_pose_quaternion");
    fp_get_nb_poses       = loadFunc<pfn_xslam_get_nb_poses>(g_hModule, XSLAM_MN_GET_NB_POSES, "xslam_get_nb_poses");
    fp_set_debug_level    = loadFunc<pfn_xslam_set_debug_level>(g_hModule, XSLAM_MN_SET_DEBUG_LEVEL, "xslam_set_debug_level");
    fp_set_coord_sys      = loadFunc<pfn_xslam_set_coordinate_system>(g_hModule, XSLAM_MN_SET_COORD_SYS, "xslam_set_coordinate_system");
    fp_json_config        = loadFunc<pfn_xslam_json_config>(g_hModule, XSLAM_MN_JSON_CONFIG, "xslam_json_config");
    fp_reset              = loadFunc<pfn_xslam_reset>(g_hModule, XSLAM_MN_RESET, "xslam_reset");
    fp_reset_slam         = loadFunc<pfn_xslam_reset_slam>(g_hModule, XSLAM_MN_RESET_SLAM, "xslam_reset_slam");
    fp_hid_write_read     = loadFunc<pfn_xslam_hid_write_read>(g_hModule, XSLAM_MN_HID_WRITE_READ, "xslam_hid_write_read");
    fp_hid_write_read_timeout = loadFunc<pfn_xslam_hid_write_read_timeout>(g_hModule, XSLAM_MN_HID_WRITE_READ_TO, "xslam_hid_write_read_timeout");
    fp_hid_write          = loadFunc<pfn_xslam_hid_write>(g_hModule, XSLAM_MN_HID_WRITE, "xslam_hid_write");
    fp_hid_read           = loadFunc<pfn_xslam_hid_read>(g_hModule, XSLAM_MN_HID_READ, "xslam_hid_read");
    fp_hid_get_report     = loadFunc<pfn_xslam_hid_get_report>(g_hModule, XSLAM_MN_HID_GET_REPORT, "xslam_hid_get_report");
    fp_host_time_now      = loadFunc<pfn_xslam_host_time_now>(g_hModule, XSLAM_MN_HOST_TIME_NOW, "xslam_host_time_now");
    fp_disp_version       = loadFunc<pfn_xslam_disp_version>(g_hModule, XSLAM_MN_DISP_VERSION, "xslam_disp_version");
    fp_has_rgb            = loadFunc<pfn_xslam_has_rgb>(g_hModule, XSLAM_MN_HAS_RGB, "xslam_has_rgb");
    fp_has_tof            = loadFunc<pfn_xslam_has_tof>(g_hModule, XSLAM_MN_HAS_TOF, "xslam_has_tof");
    fp_6dof_cb            = loadFunc<pfn_xslam_6dof_callback>(g_hModule, XSLAM_MN_6DOF_CB, "xslam_6dof_callback");
    fp_edge_6dof_cb       = loadFunc<pfn_xslam_edge_6dof_callback>(g_hModule, XSLAM_MN_EDGE_6DOF_CB, "xslam_edge_6dof_callback");
    fp_edge_6dof_quat_cb  = loadFunc<pfn_xslam_edge_6dof_quaternion_callback>(g_hModule, XSLAM_MN_EDGE_6DOF_QUAT_CB, "xslam_edge_6dof_quaternion_callback");
    fp_clear_callbacks    = loadFunc<pfn_xslam_clear_callbacks>(g_hModule, XSLAM_MN_CLEAR_CALLBACKS, "xslam_clear_callbacks");

    printf("\n");
    return (fp_init != nullptr);
}

/* -------------------------------------------------------------------------- */
/*  Helpers                                                                    */
/* -------------------------------------------------------------------------- */

static void printHex(const char* label, const unsigned char* data, int len) {
    printf("%s: ", label);
    for (int i = 0; i < len && i < 64; ++i) {
        printf("%02x ", data[i]);
    }
    printf("\n");
}

/* -------------------------------------------------------------------------- */
/*  Pose callback (for async streaming test)                                   */
/* -------------------------------------------------------------------------- */

static std::atomic<int> g_cbPoseCount{0};
static xslam_pose g_lastCbPose = {};

static void onEdgePose(xslam_pose* pose) {
    if (pose) {
        g_lastCbPose = *pose;
        g_cbPoseCount++;
    }
}

/* -------------------------------------------------------------------------- */
/*  Main test sequence                                                         */
/* -------------------------------------------------------------------------- */

int main(int argc, char* argv[]) {
    std::string csvPath;
    int durationSec = 10;
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--csv") == 0 && i + 1 < argc)
            csvPath = argv[++i];
        else if (strcmp(argv[i], "--duration") == 0 && i + 1 < argc)
            durationSec = atoi(argv[++i]);
    }

    SetConsoleCtrlHandler(consoleHandler, TRUE);

    printf("=== XSlam SDK Test Harness ===\n");
    printf("Duration: %d seconds\n\n", durationSec);

    if (!loadXSlamSDK()) {
        return 1;
    }

    // --- Step 1: Set debug level ---
    printf("[1] Setting debug level...\n");
    if (fp_set_debug_level) fp_set_debug_level(1);

    // --- Step 2: Display version ---
    printf("[2] Displaying version...\n");
    if (fp_disp_version) fp_disp_version();

    // --- Step 3: Check camera before init ---
    printf("[3] Checking camera detection...\n");
    if (fp_camera_is_detected) {
        int detected = fp_camera_is_detected();
        printf("    xslam_camera_is_detected() = %d\n", detected);
    }

    // --- Step 4: Initialize ---
    printf("[4] Initializing SDK (init_algorithm_and_loader)...\n");
    xslam_status initResult = fp_init ? fp_init() : XSLAM_ERROR;
    printf("    Result: %d (%s)\n", initResult,
           initResult == XSLAM_OK ? "OK" : "FAILED");

    if (initResult != XSLAM_OK) {
        fprintf(stderr, "ERROR: SDK initialization failed.\n");
        FreeLibrary(g_hModule);
        return 1;
    }

    // --- Step 5: Wait for camera ---
    printf("[5] Waiting for camera...\n");
    if (fp_wait_for_camera) fp_wait_for_camera();
    printf("    Camera ready.\n");

    // --- Step 6: Feature detection ---
    printf("[6] Detecting features...\n");
    if (fp_has_rgb) printf("    Has RGB: %s\n", fp_has_rgb() ? "YES" : "NO");
    if (fp_has_tof) printf("    Has ToF: %s\n", fp_has_tof() ? "YES" : "NO");

    // --- Step 7: HID test ---
    printf("[7] Testing HID write/read...\n");
    if (fp_hid_write_read) {
        // Read UUID
        unsigned char cmd[] = {0x02, 0xfd, 0x66, 0x00, 0x02};
        unsigned char resp[64] = {};
        bool ok = fp_hid_write_read(cmd, sizeof(cmd), resp, sizeof(resp));
        printf("    HID write_read(UUID) = %s\n", ok ? "OK" : "FAILED");
        if (ok) printHex("    Response", resp, 32);

        // Read version
        unsigned char vcmd[] = {0x02, 0x1c, 0x99};
        unsigned char vresp[64] = {};
        ok = fp_hid_write_read(vcmd, sizeof(vcmd), vresp, sizeof(vresp));
        printf("    HID write_read(Version) = %s\n", ok ? "OK" : "FAILED");
        if (ok) printHex("    Response", vresp, 32);

        // Read features
        unsigned char fcmd[] = {0x02, 0xde, 0x62, 0x01};
        unsigned char fresp[64] = {};
        ok = fp_hid_write_read(fcmd, sizeof(fcmd), fresp, sizeof(fresp));
        printf("    HID write_read(Features) = %s\n", ok ? "OK" : "FAILED");
        if (ok) printHex("    Response", fresp, 16);
    }

    // --- Step 8: Start camera and edge VO ---
    printf("[8] Starting camera and edge VO...\n");
    if (fp_start_camera) {
        xslam_status r = fp_start_camera();
        printf("    start_camera() = %d\n", r);
    }
    if (fp_start_edge_vo) {
        xslam_status r = fp_start_edge_vo();
        printf("    start_edge_vo() = %d\n", r);
    }

    // Register edge 6DOF callback
    if (fp_edge_6dof_cb) {
        fp_edge_6dof_cb(onEdgePose);
        printf("    Edge 6DOF callback registered.\n");
    }

    // --- Step 9: Stream pose data ---
    printf("[9] Streaming pose data for %d seconds...\n", durationSec);

    std::ofstream csvFile;
    if (!csvPath.empty()) {
        csvFile.open(csvPath);
        csvFile << "time_ms,x,y,z,r00,r01,r02,r10,r11,r12,r20,r21,r22,host_ts,edge_ts,confidence\n";
        printf("    Writing CSV to: %s\n", csvPath.c_str());
    }

    auto startTime = std::chrono::steady_clock::now();
    int pollPoseCount = 0;
    int failCount = 0;

    while (g_running) {
        auto elapsed = std::chrono::steady_clock::now() - startTime;
        if (std::chrono::duration_cast<std::chrono::seconds>(elapsed).count() >= durationSec)
            break;

        // Poll pose
        if (fp_get_pose) {
            xslam_pose pose = {};
            xslam_status r = fp_get_pose(&pose, 0.0);
            if (r == XSLAM_OK) {
                pollPoseCount++;
                double x = pose.translation[0], y = pose.translation[1], z = pose.translation[2];

                if (pollPoseCount <= 5 || pollPoseCount % 100 == 0) {
                    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
                    printf("    [%lld ms] Poll #%d: pos=[%.4f, %.4f, %.4f] edge_ts=%lld conf=%.2f\n",
                           (long long)ms, pollPoseCount, x, y, z,
                           pose.edge_timestamp_us, pose.confidence);
                }

                if (csvFile.is_open()) {
                    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count();
                    csvFile << ms;
                    for (int i = 0; i < 3; ++i) csvFile << "," << pose.translation[i];
                    for (int i = 0; i < 9; ++i) csvFile << "," << pose.rotation[i];
                    csvFile << "," << pose.host_timestamp << "," << pose.edge_timestamp_us
                            << "," << pose.confidence << "\n";
                }
            } else {
                failCount++;
            }
        }

        // Also try quaternion pose on first success
        if (pollPoseCount == 1 && fp_get_pose_quat) {
            xslam_pose_quaternion pq = {};
            xslam_status r = fp_get_pose_quat(&pq, 0.0);
            printf("    get_pose_quaternion() = %d  q=[%.4f, %.4f, %.4f, %.4f] pos=[%.4f, %.4f, %.4f]\n",
                   r, pq.quaternion[0], pq.quaternion[1], pq.quaternion[2], pq.quaternion[3],
                   pq.translation[0], pq.translation[1], pq.translation[2]);
        }

        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }

    if (csvFile.is_open()) csvFile.close();

    printf("\n[Results]\n");
    printf("    Polled poses: %d\n", pollPoseCount);
    printf("    Poll failures: %d\n", failCount);
    printf("    Callback poses: %d\n", g_cbPoseCount.load());
    if (pollPoseCount > 0) {
        auto totalMs = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - startTime).count();
        printf("    Poll rate: %.1f Hz\n", pollPoseCount * 1000.0 / totalMs);
    }
    if (g_cbPoseCount > 0) {
        auto totalMs = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - startTime).count();
        printf("    Callback rate: %.1f Hz\n", g_cbPoseCount * 1000.0 / totalMs);
        printf("    Last CB pose: [%.4f, %.4f, %.4f]\n",
               g_lastCbPose.translation[0], g_lastCbPose.translation[1], g_lastCbPose.translation[2]);
    }

    // --- Step 10: Cleanup ---
    printf("\n[10] Stopping and cleaning up...\n");
    if (fp_clear_callbacks) fp_clear_callbacks();
    if (fp_stop) {
        xslam_status r = fp_stop();
        printf("    xslam_stop() = %d\n", r);
    }
    if (fp_free) {
        xslam_status r = fp_free();
        printf("    xslam_free() = %d\n", r);
    }

    FreeLibrary(g_hModule);
    printf("\nDone.\n");
    return 0;
}
