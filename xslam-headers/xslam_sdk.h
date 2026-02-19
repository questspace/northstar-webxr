/**
 * @file xslam_sdk.h
 * @brief Reconstructed XSlam SDK API declarations
 *
 * Reverse-engineered from xslam_sdk.dll exports (1821 C++ mangled symbols).
 *
 * IMPORTANT: xslam_sdk.dll does NOT export extern "C" functions.
 * All exports use MSVC C++ name mangling. For runtime loading via
 * GetProcAddress, use the XSLAM_MANGLED_* string constants below.
 *
 * For link-time binding, declare functions WITHOUT extern "C" so the
 * compiler generates matching mangled names.
 */

#ifndef XSLAM_SDK_H
#define XSLAM_SDK_H

#include "xslam_types.h"

/* ========================================================================== */
/*  Mangled name constants for GetProcAddress (from dumpbin /EXPORTS)          */
/*                                                                             */
/*  These are the EXACT export names in xslam_sdk.dll.                         */
/* ========================================================================== */

// Lifecycle
#define XSLAM_MN_INIT               "?init_algorithm_and_loader@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_FREE               "?xslam_free@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_CAMERA_IS_DETECTED "?xslam_camera_is_detected@@YAHXZ"
#define XSLAM_MN_WAIT_FOR_CAMERA    "?xslam_wait_for_camera@@YAXXZ"
#define XSLAM_MN_START_CAMERA       "?xslam_start_camera@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_START_VO           "?xslam_start_vo@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_START_EDGE_VO      "?xslam_start_edge_vo@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_STOP               "?xslam_stop@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_CLEAR_CALLBACKS    "?xslam_clear_callbacks@@YA?AW4xslam_status@@XZ"

// Pose retrieval
#define XSLAM_MN_GET_POSE           "?xslam_get_pose@@YA?AW4xslam_status@@PEAUxslam_pose@@N@Z"
#define XSLAM_MN_GET_POSE_QUAT      "?xslam_get_pose_quaternion@@YA?AW4xslam_status@@PEAUxslam_pose_quaternion@@N@Z"
#define XSLAM_MN_GET_NB_POSES       "?xslam_get_nb_poses@@YAHXZ"

// Callbacks (function pointer overloads - NOT std::function overloads)
#define XSLAM_MN_6DOF_CB            "?xslam_6dof_callback@@YAXP6AXPEAUxslam_pose@@@Z@Z"
#define XSLAM_MN_6DOF_QUAT_CB       "?xslam_6dof_quaternion_callback@@YAXP6AXPEAUxslam_pose_quaternion@@@Z@Z"
#define XSLAM_MN_EDGE_6DOF_CB       "?xslam_edge_6dof_callback@@YAXP6AXPEAUxslam_pose@@@Z@Z"
#define XSLAM_MN_EDGE_6DOF_QUAT_CB  "?xslam_edge_6dof_quaternion_callback@@YAXP6AXPEAUxslam_pose_quaternion@@@Z@Z"
#define XSLAM_MN_IMU_CB             "?xslam_imu_callback@@YAXP6AXPEAUxslam_imu@@@Z@Z"
#define XSLAM_MN_EDGE_LOST_CB       "?xslam_edge_lost_callback@@YAXP6AXM@Z@Z"

// HID direct access
#define XSLAM_MN_HID_WRITE_READ     "?xslam_hid_write_read@@YA_NPEBEIPEAEI@Z"
#define XSLAM_MN_HID_WRITE_READ_TO  "?xslam_hid_write_read_timeout@@YA_NPEBEIPEAEII@Z"
#define XSLAM_MN_HID_WRITE          "?xslam_hid_write@@YA_NPEBEI@Z"
#define XSLAM_MN_HID_READ           "?xslam_hid_read@@YA_NPEAEI@Z"
#define XSLAM_MN_HID_GET_REPORT     "?xslam_hid_get_report@@YA_NPEAE@Z"

// Configuration
#define XSLAM_MN_SET_COORD_SYS      "?xslam_set_coordinate_system@@YAXW4xslam_coordinate_system@@@Z"
#define XSLAM_MN_SET_SDK_MODE       "?xslam_set_sdk_mode@@YAXW4xslam_sdk_mode@@@Z"
#define XSLAM_MN_SET_DEBUG_LEVEL    "?xslam_set_debug_level@@YAXH@Z"
#define XSLAM_MN_JSON_CONFIG        "?xslam_json_config@@YAXPEBD@Z"
#define XSLAM_MN_RESET              "?xslam_reset@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_RESET_SLAM         "?xslam_reset_slam@@YA?AW4xslam_status@@XZ"
#define XSLAM_MN_RESET_EDGE_SLAM    "?xslam_reset_edge_slam@@YA?AW4xslam_status@@XZ"

// Timing
#define XSLAM_MN_HOST_TIME_NOW      "?xslam_host_time_now@@YANXZ"
#define XSLAM_MN_ELAPSED_TIME       "?xslam_elapsed_time@@NANXZ"
#define XSLAM_MN_DRIVERS_TIME       "?xslam_get_drivers_time@@YANXZ"

// Stream control
#define XSLAM_MN_RGB_ENABLE         "?xslam_rgb_stream_enable@@YAXXZ"
#define XSLAM_MN_RGB_DISABLE        "?xslam_rgb_stream_disable@@YAXXZ"
#define XSLAM_MN_TOF_ENABLE         "?xslam_tof_stream_enable@@YA_NXZ"
#define XSLAM_MN_TOF_DISABLE        "?xslam_tof_stream_disable@@YA_NXZ"
#define XSLAM_MN_AUDIO_ENABLE       "?xslam_audio_stream_enable@@YAXXZ"
#define XSLAM_MN_AUDIO_DISABLE      "?xslam_audio_stream_disable@@YAXXZ"

// Feature detection
#define XSLAM_MN_HAS_RGB            "?xslam_has_rgb@@YA_NXZ"
#define XSLAM_MN_HAS_TOF            "?xslam_has_tof@@YA_NXZ"
#define XSLAM_MN_HAS_AUDIO          "?xslam_has_audio@@YA_NXZ"
#define XSLAM_MN_HAS_SPEAKER        "?xslam_has_speaker@@YA_NXZ"

// Map management
#define XSLAM_MN_SAVE_MAP           "?xslam_save_map@@YA?AW4xslam_status@@PEBD@Z"
#define XSLAM_MN_LOAD_MAP_CSLAM     "?xslam_load_map_and_switch_to_cslam@@YAXPEBDP6AXH@Z@Z"
#define XSLAM_MN_SAVE_MAP_CSLAM     "?xslam_save_map_and_switch_to_cslam@@YAXPEBDP6AXHH@Z@Z"

// Version info
#define XSLAM_MN_DISP_VERSION       "?xslam_disp_version@@YAXXZ"

/* ========================================================================== */
/*  Additional enums needed by the DLL API                                     */
/* ========================================================================== */

enum xslam_coordinate_system {
    XSLAM_COORD_RIGHT_HAND = 0,
    XSLAM_COORD_LEFT_HAND  = 1
};

enum xslam_sdk_mode {
    XSLAM_SDK_MODE_NORMAL  = 0,
    XSLAM_SDK_MODE_REPLAY  = 1,
    XSLAM_SDK_MODE_RECORD  = 2
};

/* ========================================================================== */
/*  Function pointer typedefs for runtime loading                              */
/* ========================================================================== */

// Lifecycle
typedef xslam_status (*pfn_init_algorithm_and_loader)(void);
typedef xslam_status (*pfn_xslam_free)(void);
typedef int          (*pfn_xslam_camera_is_detected)(void);
typedef void         (*pfn_xslam_wait_for_camera)(void);
typedef xslam_status (*pfn_xslam_start_camera)(void);
typedef xslam_status (*pfn_xslam_start_vo)(void);
typedef xslam_status (*pfn_xslam_start_edge_vo)(void);
typedef xslam_status (*pfn_xslam_stop)(void);
typedef xslam_status (*pfn_xslam_clear_callbacks)(void);

// Pose
typedef xslam_status (*pfn_xslam_get_pose)(xslam_pose* pose, double prediction);
typedef xslam_status (*pfn_xslam_get_pose_quaternion)(xslam_pose_quaternion* pose, double prediction);
typedef int          (*pfn_xslam_get_nb_poses)(void);

// Callbacks
typedef void (*pfn_xslam_6dof_callback)(void (*cb)(xslam_pose*));
typedef void (*pfn_xslam_6dof_quaternion_callback)(void (*cb)(xslam_pose_quaternion*));
typedef void (*pfn_xslam_edge_6dof_callback)(void (*cb)(xslam_pose*));
typedef void (*pfn_xslam_edge_6dof_quaternion_callback)(void (*cb)(xslam_pose_quaternion*));
typedef void (*pfn_xslam_imu_callback)(void (*cb)(xslam_imu*));
typedef void (*pfn_xslam_edge_lost_callback)(void (*cb)(float));

// HID
typedef bool (*pfn_xslam_hid_write_read)(const unsigned char* wdata, unsigned int wlen,
                                          unsigned char* rdata, unsigned int rlen);
typedef bool (*pfn_xslam_hid_write_read_timeout)(const unsigned char* wdata, unsigned int wlen,
                                                   unsigned char* rdata, unsigned int rlen,
                                                   unsigned int timeout_ms);
typedef bool (*pfn_xslam_hid_write)(const unsigned char* data, unsigned int len);
typedef bool (*pfn_xslam_hid_read)(unsigned char* data, unsigned int len);
typedef bool (*pfn_xslam_hid_get_report)(unsigned char* data);

// Configuration
typedef void         (*pfn_xslam_set_coordinate_system)(xslam_coordinate_system cs);
typedef void         (*pfn_xslam_set_sdk_mode)(xslam_sdk_mode mode);
typedef void         (*pfn_xslam_set_debug_level)(int level);
typedef void         (*pfn_xslam_json_config)(const char* json);
typedef xslam_status (*pfn_xslam_reset)(void);
typedef xslam_status (*pfn_xslam_reset_slam)(void);
typedef xslam_status (*pfn_xslam_reset_edge_slam)(void);

// Timing
typedef double (*pfn_xslam_host_time_now)(void);
typedef double (*pfn_xslam_elapsed_time)(void);
typedef double (*pfn_xslam_get_drivers_time)(void);

// Stream control
typedef void (*pfn_xslam_rgb_stream_enable)(void);
typedef void (*pfn_xslam_rgb_stream_disable)(void);
typedef bool (*pfn_xslam_tof_stream_enable)(void);
typedef bool (*pfn_xslam_tof_stream_disable)(void);

// Feature detection
typedef bool (*pfn_xslam_has_rgb)(void);
typedef bool (*pfn_xslam_has_tof)(void);
typedef bool (*pfn_xslam_has_audio)(void);
typedef bool (*pfn_xslam_has_speaker)(void);

// Map management
typedef xslam_status (*pfn_xslam_save_map)(const char* path);

// Version
typedef void (*pfn_xslam_disp_version)(void);

#endif /* XSLAM_SDK_H */
