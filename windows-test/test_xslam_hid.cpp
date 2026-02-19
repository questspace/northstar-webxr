/**
 * @file test_xslam_hid.cpp
 * @brief Test harness for the official XSlam HID layer (xslam-drivers.dll)
 *
 * Uses the official xslam-drivers.dll HID functions (hid_init, hid_enumerate,
 * hid_open, etc.) to communicate with the XR50. This lets us compare the
 * official HID stack behavior with our raw libusb approach.
 *
 * Supports both link-time and runtime (LoadLibrary) loading.
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <thread>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

#include "xslam_hid.h"
#include "xslam_drivers.h"

/* -------------------------------------------------------------------------- */
/*  Runtime loading                                                            */
/* -------------------------------------------------------------------------- */

#ifdef XSLAM_RUNTIME_LOAD

typedef int (*pfn_hid_init)(void);
typedef int (*pfn_hid_exit)(void);
typedef xslam_hid_device_info* (*pfn_hid_enumerate)(unsigned short, unsigned short);
typedef void (*pfn_hid_free_enumeration)(xslam_hid_device_info*);
typedef xslam_hid_device (*pfn_hid_open)(unsigned short, unsigned short, const wchar_t*);
typedef void (*pfn_hid_close)(xslam_hid_device);
typedef int (*pfn_hid_write)(xslam_hid_device, const unsigned char*, int);
typedef int (*pfn_hid_read)(xslam_hid_device, unsigned char*, int);
typedef int (*pfn_hid_read_timeout)(xslam_hid_device, unsigned char*, int, int);
typedef int (*pfn_hid_set_nonblocking)(xslam_hid_device, int);
typedef int (*pfn_hid_get_manufacturer_string)(xslam_hid_device, wchar_t*, int);
typedef int (*pfn_hid_get_product_string)(xslam_hid_device, wchar_t*, int);
typedef int (*pfn_hid_get_serial_number_string)(xslam_hid_device, wchar_t*, int);

static pfn_hid_init                     fp_hid_init = nullptr;
static pfn_hid_exit                     fp_hid_exit = nullptr;
static pfn_hid_enumerate                fp_hid_enumerate = nullptr;
static pfn_hid_free_enumeration         fp_hid_free_enumeration = nullptr;
static pfn_hid_open                     fp_hid_open = nullptr;
static pfn_hid_close                    fp_hid_close = nullptr;
static pfn_hid_write                    fp_hid_write = nullptr;
static pfn_hid_read                     fp_hid_read = nullptr;
static pfn_hid_read_timeout             fp_hid_read_timeout = nullptr;
static pfn_hid_set_nonblocking          fp_hid_set_nonblocking = nullptr;
static pfn_hid_get_manufacturer_string  fp_hid_get_manufacturer_string = nullptr;
static pfn_hid_get_product_string       fp_hid_get_product_string = nullptr;
static pfn_hid_get_serial_number_string fp_hid_get_serial_number_string = nullptr;

static HMODULE g_hDrivers = nullptr;

template<typename T>
static T loadFunc(HMODULE h, const char* name) {
    auto p = reinterpret_cast<T>(GetProcAddress(h, name));
    if (!p) fprintf(stderr, "  [WARN] Could not load: %s\n", name);
    return p;
}

static bool loadXSlamDrivers() {
    printf("Loading xslam-drivers.dll at runtime...\n");
    g_hDrivers = LoadLibraryA("xslam-drivers.dll");
    if (!g_hDrivers) {
        g_hDrivers = LoadLibraryA("../project-esky-unity/Assets/Plugins/x64/xslam-drivers.dll");
    }
    if (!g_hDrivers) {
        fprintf(stderr, "ERROR: Could not load xslam-drivers.dll (error %lu)\n", GetLastError());
        return false;
    }

    fp_hid_init                     = loadFunc<pfn_hid_init>(g_hDrivers, "hid_init");
    fp_hid_exit                     = loadFunc<pfn_hid_exit>(g_hDrivers, "hid_exit");
    fp_hid_enumerate                = loadFunc<pfn_hid_enumerate>(g_hDrivers, "hid_enumerate");
    fp_hid_free_enumeration         = loadFunc<pfn_hid_free_enumeration>(g_hDrivers, "hid_free_enumeration");
    fp_hid_open                     = loadFunc<pfn_hid_open>(g_hDrivers, "hid_open");
    fp_hid_close                    = loadFunc<pfn_hid_close>(g_hDrivers, "hid_close");
    fp_hid_write                    = loadFunc<pfn_hid_write>(g_hDrivers, "hid_write");
    fp_hid_read                     = loadFunc<pfn_hid_read>(g_hDrivers, "hid_read");
    fp_hid_read_timeout             = loadFunc<pfn_hid_read_timeout>(g_hDrivers, "hid_read_timeout");
    fp_hid_set_nonblocking          = loadFunc<pfn_hid_set_nonblocking>(g_hDrivers, "hid_set_nonblocking");
    fp_hid_get_manufacturer_string  = loadFunc<pfn_hid_get_manufacturer_string>(g_hDrivers, "hid_get_manufacturer_string");
    fp_hid_get_product_string       = loadFunc<pfn_hid_get_product_string>(g_hDrivers, "hid_get_product_string");
    fp_hid_get_serial_number_string = loadFunc<pfn_hid_get_serial_number_string>(g_hDrivers, "hid_get_serial_number_string");

    return (fp_hid_init != nullptr);
}

#define HID_CALL(fn, ...) (fp_##fn ? fp_##fn(__VA_ARGS__) : 0)
#define HID_CALL_PTR(fn, ...) (fp_##fn ? fp_##fn(__VA_ARGS__) : nullptr)
#define HID_CALL_VOID(fn, ...) do { if (fp_##fn) fp_##fn(__VA_ARGS__); } while(0)

#else // XSLAM_LINK_STATIC

#define HID_CALL(fn, ...) fn(__VA_ARGS__)
#define HID_CALL_PTR(fn, ...) fn(__VA_ARGS__)
#define HID_CALL_VOID(fn, ...) fn(__VA_ARGS__)

#endif

/* -------------------------------------------------------------------------- */
/*  Helpers                                                                    */
/* -------------------------------------------------------------------------- */

static void printHex(const char* label, const unsigned char* data, int len) {
    printf("  %s: ", label);
    for (int i = 0; i < len && i < 64; ++i) {
        printf("%02x ", data[i]);
    }
    printf("\n");
}

static void printWStr(const char* label, const wchar_t* str) {
    if (str) {
        printf("  %s: %ls\n", label, str);
    } else {
        printf("  %s: (null)\n", label);
    }
}

/* -------------------------------------------------------------------------- */
/*  Main                                                                       */
/* -------------------------------------------------------------------------- */

int main() {
    printf("=== XSlam HID Layer Test ===\n\n");

#ifdef XSLAM_RUNTIME_LOAD
    if (!loadXSlamDrivers()) {
        return 1;
    }
#endif

    // --- Step 1: Initialize HID ---
    printf("[1] Initializing HID layer...\n");
    int initRes = HID_CALL(hid_init);
    printf("  hid_init() = %d\n", initRes);

    // --- Step 2: Enumerate devices ---
    printf("\n[2] Enumerating XR50 HID devices (VID=%04x PID=%04x)...\n", XSLAM_VID, XSLAM_PID);

    auto* devs = HID_CALL_PTR(hid_enumerate, XSLAM_VID, XSLAM_PID);

    if (!devs) {
        printf("  No devices found.\n");

        // Try enumerating all HID devices
        printf("\n  Trying all HID devices (VID=0, PID=0)...\n");
        devs = HID_CALL_PTR(hid_enumerate, 0, 0);

        auto* cur = devs;
        int count = 0;
        while (cur) {
            printf("  [%d] VID=%04x PID=%04x iface=%d path=%s\n",
                   count, cur->vendor_id, cur->product_id,
                   cur->interface_number, cur->path ? cur->path : "(null)");
            cur = cur->next;
            count++;
        }
        printf("  Total HID devices: %d\n", count);

        if (devs) HID_CALL_VOID(hid_free_enumeration, devs);
        HID_CALL(hid_exit);
#ifdef XSLAM_RUNTIME_LOAD
        if (g_hDrivers) FreeLibrary(g_hDrivers);
#endif
        return 1;
    }

    // Print found devices
    auto* cur = devs;
    int devCount = 0;
    while (cur) {
        printf("  Device %d:\n", devCount);
        printf("    VID:       0x%04x\n", cur->vendor_id);
        printf("    PID:       0x%04x\n", cur->product_id);
        printf("    Interface: %d\n", cur->interface_number);
        printf("    Path:      %s\n", cur->path ? cur->path : "(null)");
        if (cur->manufacturer_string) printWStr("Mfr", cur->manufacturer_string);
        if (cur->product_string) printWStr("Product", cur->product_string);
        if (cur->serial_number) printWStr("Serial", cur->serial_number);
        cur = cur->next;
        devCount++;
    }

    // --- Step 3: Open device ---
    printf("\n[3] Opening XR50...\n");
    auto dev = HID_CALL_PTR(hid_open, XSLAM_VID, XSLAM_PID, nullptr);
    if (!dev) {
        fprintf(stderr, "ERROR: hid_open failed!\n");
        HID_CALL_VOID(hid_free_enumeration, devs);
        HID_CALL(hid_exit);
#ifdef XSLAM_RUNTIME_LOAD
        if (g_hDrivers) FreeLibrary(g_hDrivers);
#endif
        return 1;
    }
    printf("  Device opened: %p\n", dev);

    // Read device strings
    {
        wchar_t buf[256] = {};
        if (HID_CALL(hid_get_manufacturer_string, dev, buf, 256) == 0) {
            printWStr("Manufacturer", buf);
        }
        memset(buf, 0, sizeof(buf));
        if (HID_CALL(hid_get_product_string, dev, buf, 256) == 0) {
            printWStr("Product", buf);
        }
        memset(buf, 0, sizeof(buf));
        if (HID_CALL(hid_get_serial_number_string, dev, buf, 256) == 0) {
            printWStr("Serial", buf);
        }
    }

    // --- Step 4: Send HID commands ---
    printf("\n[4] Sending HID commands...\n");

    // Helper: Write + Read using official HID API
    auto hidWriteRead = [&](const char* name, const unsigned char* cmd, int cmdLen) {
        printf("\n  --- %s ---\n", name);

        // Build report: [0x02, cmd...]
        unsigned char sendBuf[64] = {};
        sendBuf[0] = 0x02;
        memcpy(sendBuf + 1, cmd, cmdLen < 63 ? cmdLen : 63);

        int written = HID_CALL(hid_write, dev, sendBuf, 64);
        printf("  hid_write: %d bytes\n", written);

        // Read response
        unsigned char recvBuf[64] = {};
        int readBytes = HID_CALL(hid_read_timeout, dev, recvBuf, 64, 2000);
        printf("  hid_read:  %d bytes\n", readBytes);
        if (readBytes > 0) {
            printHex("Response", recvBuf, readBytes < 32 ? readBytes : 32);

            // Check echo
            if (recvBuf[0] == 0x01) {
                printf("  Direction: device->host (OK)\n");
            }

            // Print ASCII content after command echo
            int dataStart = 1 + cmdLen;
            if (dataStart < readBytes) {
                printf("  Data (ASCII): \"");
                for (int i = dataStart; i < readBytes && recvBuf[i] != 0; ++i) {
                    if (recvBuf[i] >= 32 && recvBuf[i] < 127)
                        putchar(recvBuf[i]);
                    else
                        putchar('.');
                }
                printf("\"\n");
            }
        }
    };

    // UUID
    {
        unsigned char cmd[] = {0xfd, 0x66, 0x00, 0x02};
        hidWriteRead("Read UUID", cmd, sizeof(cmd));
    }

    // Version
    {
        unsigned char cmd[] = {0x1c, 0x99};
        hidWriteRead("Read Version", cmd, sizeof(cmd));
    }

    // Features
    {
        unsigned char cmd[] = {0xde, 0x62, 0x01};
        hidWriteRead("Read Features", cmd, sizeof(cmd));
    }

    // --- Step 5: Cleanup ---
    printf("\n[5] Closing device...\n");
    HID_CALL_VOID(hid_close, dev);
    HID_CALL_VOID(hid_free_enumeration, devs);
    HID_CALL(hid_exit);

#ifdef XSLAM_RUNTIME_LOAD
    if (g_hDrivers) FreeLibrary(g_hDrivers);
#endif

    printf("\nDone.\n");
    return 0;
}
