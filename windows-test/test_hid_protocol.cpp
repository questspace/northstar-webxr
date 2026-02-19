/**
 * @file test_hid_protocol.cpp
 * @brief Raw HID protocol analyzer for XVisio XR50
 *
 * Uses libusb directly (same approach as libxvisio) to:
 * 1. Find XR50 device (VID=0x040E, PID=0xF408)
 * 2. Claim interface 3
 * 3. Send all known HID commands and log responses
 * 4. Read interrupt endpoint 0x83 for SLAM packets
 * 5. Parse and display packet format
 *
 * This is the primary protocol documentation tool.
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <thread>
#include <atomic>
#include <array>
#include <vector>
#include <fstream>
#include <cmath>

#include <libusb.h>
#include "xslam_drivers.h"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

/* -------------------------------------------------------------------------- */
/*  Signal handling                                                            */
/* -------------------------------------------------------------------------- */

static std::atomic<bool> g_running{true};

#ifdef _WIN32
static BOOL WINAPI consoleHandler(DWORD signal) {
    if (signal == CTRL_C_EVENT || signal == CTRL_BREAK_EVENT) {
        g_running = false;
        return TRUE;
    }
    return FALSE;
}
#else
#include <signal.h>
static void signalHandler(int) { g_running = false; }
#endif

/* -------------------------------------------------------------------------- */
/*  Helpers                                                                    */
/* -------------------------------------------------------------------------- */

static void printHex(const char* label, const unsigned char* data, int len) {
    printf("  %s (%d bytes):\n    ", label, len);
    for (int i = 0; i < len; ++i) {
        printf("%02x ", data[i]);
        if ((i + 1) % 16 == 0 && i + 1 < len) printf("\n    ");
    }
    printf("\n");
}

static void printAscii(const char* label, const unsigned char* data, int len) {
    printf("  %s: \"", label);
    for (int i = 0; i < len && data[i] != 0; ++i) {
        if (data[i] >= 32 && data[i] < 127)
            putchar(data[i]);
        else
            putchar('.');
    }
    printf("\"\n");
}

/**
 * Execute an HID transaction: SET_REPORT followed by GET_REPORT.
 * This mirrors the libxvisio HID::executeTransaction implementation.
 */
static bool hidTransaction(libusb_device_handle* handle,
                            const unsigned char* cmd, int cmdLen,
                            unsigned char* response, int respLen,
                            unsigned int timeout = 1000) {
    constexpr uint8_t reqType = LIBUSB_REQUEST_TYPE_CLASS | LIBUSB_RECIPIENT_INTERFACE;
    constexpr int reportSize = XSLAM_HID_REPORT_SIZE;

    // Build SET_REPORT payload: [0x02, cmd...]
    std::array<unsigned char, 63> sendBuf = {};
    sendBuf[0] = 0x02;  // Host -> Device direction
    memcpy(sendBuf.data() + 1, cmd, cmdLen < 62 ? cmdLen : 62);

    int res = libusb_control_transfer(
        handle,
        LIBUSB_ENDPOINT_OUT | reqType,
        XSLAM_HID_SET_REPORT,
        XSLAM_HID_REPORT_TYPE_OUTPUT,
        XSLAM_HID_INTERFACE,
        sendBuf.data(), reportSize,
        timeout);

    if (res < 0) {
        fprintf(stderr, "    SET_REPORT failed: %s\n", libusb_strerror((libusb_error)res));
        return false;
    }

    // GET_REPORT
    std::array<unsigned char, 63> recvBuf = {};
    res = libusb_control_transfer(
        handle,
        LIBUSB_ENDPOINT_IN | reqType,
        XSLAM_HID_GET_REPORT,
        XSLAM_HID_REPORT_TYPE_INPUT,
        XSLAM_HID_INTERFACE,
        recvBuf.data(), reportSize,
        timeout);

    if (res < 0) {
        fprintf(stderr, "    GET_REPORT failed: %s\n", libusb_strerror((libusb_error)res));
        return false;
    }

    // Verify response echo
    if (recvBuf[0] != 0x01) {
        fprintf(stderr, "    Response direction byte: 0x%02x (expected 0x01)\n", recvBuf[0]);
        return false;
    }

    // Check command echo
    bool cmdMatch = true;
    for (int i = 0; i < cmdLen && i < 62; ++i) {
        if (sendBuf[i + 1] != recvBuf[i + 1]) {
            cmdMatch = false;
            break;
        }
    }

    // Copy response data
    int dataStart = 1 + cmdLen;
    int copyLen = (reportSize - dataStart) < respLen ? (reportSize - dataStart) : respLen;
    if (copyLen > 0) {
        memcpy(response, recvBuf.data() + dataStart, copyLen);
    }

    return cmdMatch;
}

/* -------------------------------------------------------------------------- */
/*  SLAM packet parser                                                         */
/* -------------------------------------------------------------------------- */

struct SlamPacket {
    uint32_t timestamp;
    double translation[3];
    double rotation[9];
};

static SlamPacket parseSlamPacket(const unsigned char* buffer) {
    SlamPacket pkt = {};

    pkt.timestamp = *reinterpret_cast<const uint32_t*>(&buffer[XSLAM_PKT_TIMESTAMP_OFFSET]);

    const auto* transInts = reinterpret_cast<const int32_t*>(&buffer[XSLAM_PKT_TRANSLATION_OFFSET]);
    for (int i = 0; i < 3; ++i) {
        pkt.translation[i] = transInts[i] * XSLAM_FLOAT_SCALE;
    }

    const auto* rotShorts = reinterpret_cast<const int16_t*>(&buffer[XSLAM_PKT_ROTATION_OFFSET]);
    for (int i = 0; i < 9; ++i) {
        pkt.rotation[i] = rotShorts[i] * XSLAM_FLOAT_SCALE;
    }

    return pkt;
}

/* -------------------------------------------------------------------------- */
/*  Main                                                                       */
/* -------------------------------------------------------------------------- */

int main(int argc, char* argv[]) {
    int durationSec = 10;
    std::string csvPath;
    bool dumpRaw = false;

    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--duration") == 0 && i + 1 < argc)
            durationSec = atoi(argv[++i]);
        else if (strcmp(argv[i], "--csv") == 0 && i + 1 < argc)
            csvPath = argv[++i];
        else if (strcmp(argv[i], "--raw") == 0)
            dumpRaw = true;
    }

#ifdef _WIN32
    SetConsoleCtrlHandler(consoleHandler, TRUE);
#else
    signal(SIGINT, signalHandler);
#endif

    printf("=== XVisio XR50 HID Protocol Analyzer ===\n\n");

    // --- Initialize libusb ---
    libusb_context* ctx = nullptr;
    int res = libusb_init(&ctx);
    if (res != 0) {
        fprintf(stderr, "ERROR: libusb_init failed: %s\n", libusb_strerror((libusb_error)res));
        return 1;
    }

    // --- Find XR50 ---
    printf("[1] Searching for XR50 (VID=%04x PID=%04x)...\n", XSLAM_VID, XSLAM_PID);

    libusb_device** devList = nullptr;
    ssize_t devCount = libusb_get_device_list(ctx, &devList);
    libusb_device* xr50Device = nullptr;

    for (ssize_t i = 0; i < devCount; ++i) {
        libusb_device_descriptor desc = {};
        libusb_get_device_descriptor(devList[i], &desc);
        if (desc.idVendor == XSLAM_VID && desc.idProduct == XSLAM_PID) {
            xr50Device = devList[i];
            printf("  Found XR50 at bus %d, port %d\n",
                   libusb_get_bus_number(xr50Device),
                   libusb_get_port_number(xr50Device));
            break;
        }
    }

    if (!xr50Device) {
        fprintf(stderr, "ERROR: XR50 not found. Is it plugged in?\n");
        libusb_free_device_list(devList, 1);
        libusb_exit(ctx);
        return 1;
    }

    // --- Open device ---
    printf("[2] Opening device...\n");
    libusb_device_handle* handle = nullptr;
    res = libusb_open(xr50Device, &handle);
    if (res != 0) {
        fprintf(stderr, "ERROR: libusb_open failed: %s\n", libusb_strerror((libusb_error)res));
        libusb_free_device_list(devList, 1);
        libusb_exit(ctx);
        return 1;
    }

    // --- Dump device descriptor ---
    {
        libusb_device_descriptor desc = {};
        libusb_get_device_descriptor(xr50Device, &desc);
        printf("  Vendor ID:  0x%04x\n", desc.idVendor);
        printf("  Product ID: 0x%04x\n", desc.idProduct);
        printf("  Class:      %d\n", desc.bDeviceClass);
        printf("  Configs:    %d\n", desc.bNumConfigurations);
    }

    // --- Dump config descriptor ---
    printf("[3] Enumerating interfaces...\n");
    {
        libusb_config_descriptor* cfg = nullptr;
        libusb_get_config_descriptor(xr50Device, 0, &cfg);
        printf("  Configuration %d: %d interfaces\n", cfg->bConfigurationValue, cfg->bNumInterfaces);
        for (int i = 0; i < cfg->bNumInterfaces; ++i) {
            const auto& iface = cfg->interface[i];
            for (int j = 0; j < iface.num_altsetting; ++j) {
                const auto& alt = iface.altsetting[j];
                printf("    Interface %d alt %d: class=%d subclass=%d protocol=%d endpoints=%d\n",
                       alt.bInterfaceNumber, alt.bAlternateSetting,
                       alt.bInterfaceClass, alt.bInterfaceSubClass,
                       alt.bInterfaceProtocol, alt.bNumEndpoints);
                for (int k = 0; k < alt.bNumEndpoints; ++k) {
                    const auto& ep = alt.endpoint[k];
                    printf("      EP 0x%02x: type=%d maxPacket=%d interval=%d\n",
                           ep.bEndpointAddress, ep.bmAttributes & 0x03,
                           ep.wMaxPacketSize, ep.bInterval);
                }
            }
        }
        libusb_free_config_descriptor(cfg);
    }

    // --- Claim HID interface ---
    printf("[4] Claiming interface %d...\n", XSLAM_HID_INTERFACE);

#ifdef __linux__
    if (libusb_kernel_driver_active(handle, XSLAM_HID_INTERFACE) == 1) {
        printf("  Detaching kernel driver...\n");
        libusb_detach_kernel_driver(handle, XSLAM_HID_INTERFACE);
    }
#endif

    res = libusb_claim_interface(handle, XSLAM_HID_INTERFACE);
    if (res != 0) {
        fprintf(stderr, "ERROR: claim interface failed: %s\n", libusb_strerror((libusb_error)res));
        libusb_close(handle);
        libusb_free_device_list(devList, 1);
        libusb_exit(ctx);
        return 1;
    }
    printf("  Interface claimed.\n");

    // --- Test HID commands ---
    printf("\n[5] Testing HID commands...\n\n");

    // 5a. UUID
    printf("--- Command: Read UUID ---\n");
    {
        unsigned char cmd[] = {0xfd, 0x66, 0x00, 0x02};
        unsigned char resp[58] = {};
        bool ok = hidTransaction(handle, cmd, sizeof(cmd), resp, sizeof(resp));
        printf("  Result: %s\n", ok ? "OK" : "FAILED");
        if (ok) {
            printAscii("UUID", resp, sizeof(resp));
            printHex("Raw", resp, 32);
        }
    }

    // 5b. Version
    printf("\n--- Command: Read Version ---\n");
    {
        unsigned char cmd[] = {0x1c, 0x99};
        unsigned char resp[60] = {};
        bool ok = hidTransaction(handle, cmd, sizeof(cmd), resp, sizeof(resp));
        printf("  Result: %s\n", ok ? "OK" : "FAILED");
        if (ok) {
            printAscii("Version", resp, sizeof(resp));
            printHex("Raw", resp, 32);
        }
    }

    // 5c. Features
    printf("\n--- Command: Read Features ---\n");
    {
        unsigned char cmd[] = {0xde, 0x62, 0x01};
        unsigned char resp[59] = {};
        bool ok = hidTransaction(handle, cmd, sizeof(cmd), resp, sizeof(resp));
        printf("  Result: %s\n", ok ? "OK" : "FAILED");
        if (ok) {
            uint32_t features = resp[0] | (resp[1] << 8) | (resp[2] << 16) | (resp[3] << 24);
            printf("  Features bitmap: 0x%08x\n", features);
            printf("    Edge mode:     %s\n", (features & XSLAM_FEATURE_EDGE_MODE) ? "YES" : "NO");
            printf("    Mixed mode:    %s\n", (features & XSLAM_FEATURE_MIXED_MODE) ? "YES" : "NO");
            printf("    Stereo:        %s\n", (features & XSLAM_FEATURE_STEREO) ? "YES" : "NO");
            printf("    RGB:           %s\n", (features & XSLAM_FEATURE_RGB) ? "YES" : "NO");
            printf("    ToF:           %s\n", (features & XSLAM_FEATURE_TOF) ? "YES" : "NO");
            printf("    SGBM:          %s\n", (features & XSLAM_FEATURE_SGBM) ? "YES" : "NO");
            printf("    Eye tracking:  %s\n", (features & XSLAM_FEATURE_EYE_TRACKING) ? "YES" : "NO");
            printHex("Raw", resp, 16);
        }
    }

    // 5d. Configure device (Edge 6DOF, UVC mode 1, no embedded algo)
    printf("\n--- Command: Configure Device (Edge mode) ---\n");
    {
        unsigned char cmd[] = {0x19, 0x95, 0x01, 0x01, 0x00};
        unsigned char resp[57] = {};
        bool ok = hidTransaction(handle, cmd, sizeof(cmd), resp, sizeof(resp));
        printf("  Result: %s\n", ok ? "OK" : "FAILED");
        printHex("Raw", resp, 16);
    }

    // 5e. Start edge stream
    printf("\n--- Command: Start Edge Stream ---\n");
    {
        unsigned char cmd[] = {0xa2, 0x33, 0x01, 0x00, 0x00};
        unsigned char resp[57] = {};
        bool ok = hidTransaction(handle, cmd, sizeof(cmd), resp, sizeof(resp));
        printf("  Result: %s\n", ok ? "OK" : "FAILED");
        printHex("Raw", resp, 16);
    }

    // --- Read SLAM packets ---
    printf("\n[6] Reading SLAM packets from EP 0x83 (%d seconds)...\n\n", durationSec);

    std::ofstream csvFile;
    if (!csvPath.empty()) {
        csvFile.open(csvPath);
        csvFile << "packet_num,timestamp,tx,ty,tz,r00,r01,r02,r10,r11,r12,r20,r21,r22\n";
        printf("  Writing CSV to: %s\n", csvPath.c_str());
    }

    auto startTime = std::chrono::steady_clock::now();
    int packetCount = 0;
    int errorCount = 0;

    while (g_running) {
        auto elapsed = std::chrono::steady_clock::now() - startTime;
        if (std::chrono::duration_cast<std::chrono::seconds>(elapsed).count() >= durationSec) {
            break;
        }

        unsigned char buffer[64] = {};
        int transferred = 0;
        res = libusb_interrupt_transfer(handle, XSLAM_SLAM_ENDPOINT, buffer, 64,
                                         &transferred, 1000);

        if (res == LIBUSB_SUCCESS && transferred > 0) {
            packetCount++;

            if (dumpRaw || packetCount <= 3) {
                printf("  Packet #%d (%d bytes):\n", packetCount, transferred);
                printHex("Raw", buffer, transferred);
            }

            SlamPacket pkt = parseSlamPacket(buffer);

            if (packetCount <= 10 || packetCount % 100 == 0) {
                printf("  #%d  ts=%u  pos=[%.4f, %.4f, %.4f]  rot_diag=[%.4f, %.4f, %.4f]\n",
                       packetCount, pkt.timestamp,
                       pkt.translation[0], pkt.translation[1], pkt.translation[2],
                       pkt.rotation[0], pkt.rotation[4], pkt.rotation[8]);
            }

            if (csvFile.is_open()) {
                csvFile << packetCount << "," << pkt.timestamp;
                for (int i = 0; i < 3; ++i) csvFile << "," << pkt.translation[i];
                for (int i = 0; i < 9; ++i) csvFile << "," << pkt.rotation[i];
                csvFile << "\n";
            }

        } else if (res == LIBUSB_ERROR_TIMEOUT) {
            // Timeout is normal if no data available
        } else if (res != LIBUSB_SUCCESS) {
            errorCount++;
            if (errorCount <= 5) {
                fprintf(stderr, "  EP read error: %s\n", libusb_strerror((libusb_error)res));
            }
        }
    }

    printf("\n[Results]\n");
    printf("  Total packets: %d\n", packetCount);
    printf("  Errors: %d\n", errorCount);
    if (packetCount > 0) {
        auto totalMs = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - startTime).count();
        printf("  Avg rate: %.1f Hz\n", packetCount * 1000.0 / totalMs);
    }

    if (csvFile.is_open()) csvFile.close();

    // --- Cleanup ---
    printf("\n[7] Releasing interface...\n");
    libusb_release_interface(handle, XSLAM_HID_INTERFACE);
    libusb_close(handle);
    libusb_free_device_list(devList, 1);
    libusb_exit(ctx);

    printf("Done.\n");
    return 0;
}
