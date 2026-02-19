/**
 * @file xslam_drivers.h
 * @brief Reconstructed XSlam driver layer declarations
 *
 * Reverse-engineered from xslam-drivers.dll exports.
 * The drivers layer handles low-level USB/HID communication with the XR50.
 *
 * The XSlam driver architecture:
 *   xslam_sdk.dll  ->  xslam-drivers.dll  ->  USB/HID (libusb)
 *                                          ->  UVC (camera streams)
 *                                          ->  VSC (vendor specific class)
 *
 * The drivers DLL provides:
 *   - HID device enumeration and communication
 *   - USB bulk/interrupt transfer management
 *   - Camera stream handling (UVC)
 *   - Device configuration via HID control transfers
 */

#ifndef XSLAM_DRIVERS_H
#define XSLAM_DRIVERS_H

#include "xslam_types.h"

#ifdef _WIN32
  #ifdef XSLAM_DRIVERS_EXPORTS
    #define XSLAM_DRV_API __declspec(dllexport)
  #else
    #define XSLAM_DRV_API __declspec(dllimport)
  #endif
#else
  #define XSLAM_DRV_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ========================================================================== */
/*  XR50 Device Constants                                                      */
/* ========================================================================== */

/** XVisio XR50 USB Vendor ID */
#define XSLAM_VID           0x040E

/** XVisio XR50 USB Product ID */
#define XSLAM_PID           0xF408

/** HID interface number (interface 3 on the XR50) */
#define XSLAM_HID_INTERFACE 3

/** SLAM interrupt endpoint (endpoint 0x83 = EP3 IN) */
#define XSLAM_SLAM_ENDPOINT 0x83

/** HID report sizes */
#define XSLAM_HID_REPORT_SIZE    63
#define XSLAM_HID_SEND_SIZE      64

/** HID control transfer values (from libxvisio HID implementation) */
#define XSLAM_HID_SET_REPORT     0x09
#define XSLAM_HID_GET_REPORT     0x01
#define XSLAM_HID_REPORT_TYPE_OUTPUT  0x0202
#define XSLAM_HID_REPORT_TYPE_INPUT   0x0101

/* ========================================================================== */
/*  Known HID Commands                                                         */
/*                                                                             */
/*  Commands are sent as: [0x02, cmd_byte_1, cmd_byte_2, ...]                  */
/*  The 0x02 prefix indicates direction (host->device).                        */
/*  Response has 0x01 prefix (device->host).                                   */
/* ========================================================================== */

/** Read device UUID. Command: {0xfd, 0x66, 0x00, 0x02} */
#define XSLAM_CMD_UUID_0            0xFD
#define XSLAM_CMD_UUID_1            0x66

/** Read firmware version. Command: {0x1c, 0x99} */
#define XSLAM_CMD_VERSION_0         0x1C
#define XSLAM_CMD_VERSION_1         0x99

/** Read device features bitmap. Command: {0xde, 0x62, 0x01} */
#define XSLAM_CMD_FEATURES_0        0xDE
#define XSLAM_CMD_FEATURES_1        0x62

/**
 * Configure device mode.
 * Command: {0x19, 0x95, edge6dof, uvcMode, embeddedAlgo}
 *   edge6dof:     1 = edge SLAM, 0 = host SLAM
 *   uvcMode:      UVC streaming mode (typically 1)
 *   embeddedAlgo: 1 = embedded algorithm (mixed mode)
 */
#define XSLAM_CMD_CONFIGURE_0       0x19
#define XSLAM_CMD_CONFIGURE_1       0x95

/**
 * Start edge SLAM stream.
 * Command: {0xa2, 0x33, edgeMode, rotationEnabled, flipped}
 *   edgeMode:        1 = enable edge mode streaming
 *   rotationEnabled: 1 = include rotation in packets
 *   flipped:         1 = flip coordinate system
 */
#define XSLAM_CMD_EDGE_STREAM_0     0xA2
#define XSLAM_CMD_EDGE_STREAM_1     0x33

/* ========================================================================== */
/*  Features Bitmap (from device.cpp feature query)                            */
/* ========================================================================== */

#define XSLAM_FEATURE_EDGE_MODE      (1 << 0)
#define XSLAM_FEATURE_MIXED_MODE     (1 << 1)
#define XSLAM_FEATURE_STEREO         (1 << 2)
#define XSLAM_FEATURE_RGB            (1 << 3)
#define XSLAM_FEATURE_TOF            (1 << 4)
#define XSLAM_FEATURE_IA             (1 << 5)
#define XSLAM_FEATURE_SGBM           (1 << 6)
#define XSLAM_FEATURE_EYE_TRACKING   (1 << 10)
#define XSLAM_FEATURE_FACE_ID        (1 << 12)

/* ========================================================================== */
/*  SLAM Packet Format                                                         */
/*                                                                             */
/*  Received on endpoint 0x83 as 64-byte interrupt transfers.                  */
/*  Packet layout (from slam.cpp):                                             */
/*    [0..2]  Header / packet type                                             */
/*    [3..6]  uint32_t timestamp (edge time, units TBD)                        */
/*    [7..18] int32_t translation[3] (scaled by float_scale = 6.1035e-05)      */
/*    [19..36] int16_t rotation[9] (3x3 matrix, scaled by float_scale)         */
/*    [37..63] Additional data (TBD - may include velocity, status)            */
/* ========================================================================== */

/** Scale factor for converting raw int16/int32 to float */
#define XSLAM_FLOAT_SCALE   6.103515625e-05

/** Offsets within a 64-byte SLAM packet */
#define XSLAM_PKT_TIMESTAMP_OFFSET    3
#define XSLAM_PKT_TRANSLATION_OFFSET  7
#define XSLAM_PKT_ROTATION_OFFSET     19

/* ========================================================================== */
/*  Driver-level functions (from xslam-drivers.dll)                            */
/*                                                                             */
/*  Note: These are best-effort reconstructions. The actual xslam-drivers.dll  */
/*  may use C++ name mangling for some exports. Use dumpbin /exports to verify.*/
/* ========================================================================== */

/**
 * Perform a HID write followed by read on the XR50.
 * This is the primary mechanism for sending commands and receiving responses.
 *
 * @param wdata   Data to write (HID report)
 * @param wlen    Length of write data
 * @param rdata   Buffer for response data
 * @param rlen    Length of response buffer
 * @return        Non-zero on success
 */
XSLAM_DRV_API int xslam_hid_write_read(unsigned char* wdata, int wlen,
                                         unsigned char* rdata, int rlen);

#ifdef __cplusplus
}
#endif

#endif /* XSLAM_DRIVERS_H */
