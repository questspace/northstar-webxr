/**
 * @file xslam_hid.h
 * @brief Reconstructed HID layer declarations
 *
 * The xslam-drivers.dll includes a HID implementation similar to hidapi.
 * These functions handle low-level HID device enumeration and communication.
 *
 * The XR50 uses USB HID interface 3 for control commands and
 * interrupt endpoint 0x83 for streaming SLAM data.
 *
 * HID Protocol:
 *   - SET_REPORT (0x09): Send 63-byte command to device
 *     wValue=0x0202, wIndex=3 (interface)
 *     Data: [0x02, cmd...] (0x02 = host->device direction)
 *
 *   - GET_REPORT (0x01): Read 63-byte response from device
 *     wValue=0x0101, wIndex=3 (interface)
 *     Response: [0x01, echo_cmd..., data...] (0x01 = device->host)
 *
 *   - Interrupt IN (EP 0x83): Streaming SLAM packets (64 bytes each)
 */

#ifndef XSLAM_HID_H
#define XSLAM_HID_H

#include <stdint.h>

#ifdef _WIN32
  #ifdef XSLAM_DRIVERS_EXPORTS
    #define XSLAM_HID_API __declspec(dllexport)
  #else
    #define XSLAM_HID_API __declspec(dllimport)
  #endif
#else
  #define XSLAM_HID_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ========================================================================== */
/*  HID Device Info                                                            */
/* ========================================================================== */

/**
 * HID device information structure.
 * Returned by hid_enumerate(), linked list.
 */
typedef struct xslam_hid_device_info {
    char* path;                           /**< Platform-specific device path */
    unsigned short vendor_id;             /**< USB Vendor ID */
    unsigned short product_id;            /**< USB Product ID */
    wchar_t* serial_number;               /**< Serial number string */
    unsigned short release_number;        /**< Device release number */
    wchar_t* manufacturer_string;         /**< Manufacturer string */
    wchar_t* product_string;              /**< Product string */
    unsigned short usage_page;            /**< HID Usage Page */
    unsigned short usage;                 /**< HID Usage */
    int interface_number;                 /**< USB interface number */
    struct xslam_hid_device_info* next;   /**< Next device in linked list */
} xslam_hid_device_info;

/** Opaque HID device handle */
typedef void* xslam_hid_device;

/* ========================================================================== */
/*  HID API (hidapi-compatible interface from xslam-drivers.dll)               */
/* ========================================================================== */

/** Initialize the HID library. Call before any other HID functions. */
XSLAM_HID_API int hid_init(void);

/** Finalize the HID library. Call when done with HID operations. */
XSLAM_HID_API int hid_exit(void);

/**
 * Enumerate HID devices.
 * @param vendor_id   Filter by vendor ID (0 = any)
 * @param product_id  Filter by product ID (0 = any)
 * @return Linked list of matching devices, or NULL.
 */
XSLAM_HID_API xslam_hid_device_info* hid_enumerate(unsigned short vendor_id,
                                                      unsigned short product_id);

/** Free a device info list returned by hid_enumerate(). */
XSLAM_HID_API void hid_free_enumeration(xslam_hid_device_info* devs);

/**
 * Open a HID device by vendor/product ID.
 * @param vendor_id   USB Vendor ID
 * @param product_id  USB Product ID
 * @param serial_number  Serial number (NULL for any)
 * @return Device handle, or NULL on failure.
 */
XSLAM_HID_API xslam_hid_device hid_open(unsigned short vendor_id,
                                           unsigned short product_id,
                                           const wchar_t* serial_number);

/** Open a HID device by path string. */
XSLAM_HID_API xslam_hid_device hid_open_path(const char* path);

/** Close a HID device. */
XSLAM_HID_API void hid_close(xslam_hid_device dev);

/**
 * Write data to a HID device (SET_REPORT).
 * @param dev    Device handle
 * @param data   Data to write (first byte is report ID)
 * @param length Data length
 * @return Bytes written, or -1 on error.
 */
XSLAM_HID_API int hid_write(xslam_hid_device dev, const unsigned char* data,
                               int length);

/**
 * Read data from a HID device.
 * @param dev    Device handle
 * @param data   Buffer for received data
 * @param length Buffer size
 * @return Bytes read, or -1 on error.
 */
XSLAM_HID_API int hid_read(xslam_hid_device dev, unsigned char* data, int length);

/**
 * Read with timeout.
 * @param dev     Device handle
 * @param data    Buffer for received data
 * @param length  Buffer size
 * @param milliseconds  Timeout in ms (-1 = blocking, 0 = non-blocking)
 * @return Bytes read, 0 on timeout, -1 on error.
 */
XSLAM_HID_API int hid_read_timeout(xslam_hid_device dev, unsigned char* data,
                                      int length, int milliseconds);

/**
 * Send a feature report.
 * @param dev    Device handle
 * @param data   Feature report data (first byte is report ID)
 * @param length Data length
 * @return Bytes sent, or -1 on error.
 */
XSLAM_HID_API int hid_send_feature_report(xslam_hid_device dev,
                                             const unsigned char* data, int length);

/**
 * Get a feature report.
 * @param dev    Device handle
 * @param data   Buffer (first byte is report ID)
 * @param length Buffer size
 * @return Bytes received, or -1 on error.
 */
XSLAM_HID_API int hid_get_feature_report(xslam_hid_device dev,
                                            unsigned char* data, int length);

/** Set nonblocking mode (0=blocking, 1=nonblocking). */
XSLAM_HID_API int hid_set_nonblocking(xslam_hid_device dev, int nonblock);

/** Get last error string. */
XSLAM_HID_API const wchar_t* hid_error(xslam_hid_device dev);

/** Get manufacturer string. */
XSLAM_HID_API int hid_get_manufacturer_string(xslam_hid_device dev,
                                                 wchar_t* string, int maxlen);

/** Get product string. */
XSLAM_HID_API int hid_get_product_string(xslam_hid_device dev,
                                            wchar_t* string, int maxlen);

/** Get serial number string. */
XSLAM_HID_API int hid_get_serial_number_string(xslam_hid_device dev,
                                                  wchar_t* string, int maxlen);

#ifdef __cplusplus
}
#endif

#endif /* XSLAM_HID_H */
