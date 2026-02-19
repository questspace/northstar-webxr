/**
 * @file xslam_types.h
 * @brief Reconstructed XSlam SDK data types
 *
 * Reverse-engineered from:
 *   - xslam_sdk.dll / xslam-drivers.dll exports (Project Esky, Windows x64)
 *   - xv-types.h / xv-sdk.h (XVisio Android SDK)
 *   - unity-wrapper.h (XVisio Unity integration)
 *
 * These types mirror the C-style structs used by the xslam_* functions
 * exported from the official Windows DLLs.
 */

#ifndef XSLAM_TYPES_H
#define XSLAM_TYPES_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------- */
/*  Status codes                                                               */
/* -------------------------------------------------------------------------- */

typedef enum xslam_status {
    XSLAM_OK              = 0,
    XSLAM_ERROR           = 1,
    XSLAM_NOT_INITIALIZED = 2,
    XSLAM_NO_DEVICE       = 3,
    XSLAM_TIMEOUT         = 4,
    XSLAM_INVALID_PARAM   = 5
} xslam_status;

/* -------------------------------------------------------------------------- */
/*  Basic vector / matrix types (match unity-wrapper.h layout)                 */
/* -------------------------------------------------------------------------- */

typedef struct xslam_vector2 {
    float x, y;
} xslam_vector2;

typedef struct xslam_vector3 {
    float x, y, z;
} xslam_vector3;

typedef struct xslam_vector3_uint {
    unsigned int x, y, z;
} xslam_vector3_uint;

typedef struct xslam_vector4 {
    float x, y, z, w;
} xslam_vector4;

typedef struct xslam_point {
    double x, y, z;
} xslam_point;

typedef struct xslam_matrix4x4 {
    float m[16];
} xslam_matrix4x4;

/* -------------------------------------------------------------------------- */
/*  Pose types                                                                 */
/* -------------------------------------------------------------------------- */

/**
 * 6-DOF pose with rotation matrix (row-major 3x3) and translation.
 * Matches xv::Pose layout: translation[3], rotation[9], hostTimestamp, edgeTimestamp.
 */
typedef struct xslam_pose {
    double rotation[9];       /**< 3x3 row-major rotation matrix */
    double translation[3];    /**< x, y, z translation */
    double host_timestamp;    /**< Host timestamp in seconds (steady_clock) */
    int64_t edge_timestamp_us;/**< Edge timestamp in microseconds */
    double confidence;        /**< Confidence [0,1], 0 = lost */
} xslam_pose;

/**
 * 6-DOF pose with quaternion rotation.
 */
typedef struct xslam_pose_quaternion {
    double quaternion[4];     /**< qx, qy, qz, qw */
    double translation[3];    /**< x, y, z */
    double host_timestamp;    /**< Host timestamp in seconds */
    int64_t edge_timestamp_us;/**< Edge timestamp in microseconds */
    double confidence;        /**< Confidence [0,1] */
} xslam_pose_quaternion;

/**
 * 3-DOF orientation (rotation only).
 * Matches unity-wrapper.h Orientation struct.
 */
typedef struct xslam_orientation {
    long long host_timestamp;  /**< Timestamp in us on host */
    long long device_timestamp;/**< Timestamp in us on device */
    double qx, qy, qz, qw;   /**< Absolute quaternion (3DoF) */
    double roll, pitch, yaw;   /**< Euler angles (3DoF) */
    double angular_velocity[3];/**< Instantaneous angular velocity (rad/s) */
} xslam_orientation;

/* -------------------------------------------------------------------------- */
/*  IMU types                                                                  */
/* -------------------------------------------------------------------------- */

typedef struct xslam_imu {
    xslam_vector3 accel;      /**< 3-axis accelerometer (m/s^2) */
    xslam_vector3 gyro;       /**< 3-axis gyrometer (rad/s) */
    xslam_vector3 magneto;    /**< 3-axis magnetometer */
    long long timestamp;       /**< Edge timestamp in microseconds */
} xslam_imu;

/* -------------------------------------------------------------------------- */
/*  Calibration types (from unity-wrapper.h)                                   */
/* -------------------------------------------------------------------------- */

typedef struct xslam_transform {
    double rotation[9];       /**< 3x3 row-major rotation matrix */
    double translation[3];    /**< Translation vector */
} xslam_transform;

typedef struct xslam_pdm {
    double K[11];
    /**
     * K[0]=fx, K[1]=fy, K[2]=u0, K[3]=v0,
     * K[4]=k1, K[5]=k2, K[6]=p1, K[7]=p2, K[8]=k3,
     * K[9]=width, K[10]=height
     */
} xslam_pdm;

typedef struct xslam_ucm {
    double K[7];
    /**
     * K[0]=fx, K[1]=fy, K[2]=u0, K[3]=v0, K[4]=xi,
     * K[5]=width, K[6]=height
     */
} xslam_ucm;

typedef struct xslam_ucm_calibration {
    xslam_transform extrinsic;
    xslam_ucm intrinsic;
} xslam_ucm_calibration;

typedef struct xslam_stereo_fisheyes {
    xslam_ucm_calibration calibrations[2];
} xslam_stereo_fisheyes;

typedef struct xslam_pdm_calibration {
    xslam_transform extrinsic;
    xslam_pdm intrinsic;
} xslam_pdm_calibration;

typedef struct xslam_stereo_pdm_calibration {
    xslam_pdm_calibration calibrations[2];
} xslam_stereo_pdm_calibration;

typedef struct xslam_rgb_calibration {
    xslam_transform extrinsic;
    xslam_pdm intrinsic1080; /**< 1920x1080 */
    xslam_pdm intrinsic720;  /**< 1280x720 */
    xslam_pdm intrinsic480;  /**< 640x480 */
} xslam_rgb_calibration;

typedef struct xslam_imu_bias {
    double gyro_offset[3];
    double accel_offset[3];
} xslam_imu_bias;

/* -------------------------------------------------------------------------- */
/*  SLAM enumerations                                                          */
/* -------------------------------------------------------------------------- */

typedef enum xslam_slam_type {
    XSLAM_SLAM_EDGE             = 0,
    XSLAM_SLAM_MIXED            = 1,
    XSLAM_SLAM_EDGE_FUSION_HOST = 2
} xslam_slam_type;

typedef enum xslam_component {
    XSLAM_COM_ALL    = 0xFFFF,
    XSLAM_COM_IMU    = 0x0001,
    XSLAM_COM_POSE   = 0x0002,
    XSLAM_COM_STEREO = 0x0004,
    XSLAM_COM_RGB    = 0x0008,
    XSLAM_COM_TOF    = 0x0010,
    XSLAM_COM_EVENTS = 0x0040,
    XSLAM_COM_CNN    = 0x0080,
    XSLAM_COM_HID    = 0x0100,
    XSLAM_COM_UVC    = 0x0200,
    XSLAM_COM_VSC    = 0x0400,
    XSLAM_COM_SLAM   = 0x0800,
    XSLAM_COM_EDGEP  = 0x1000
} xslam_component;

typedef enum xslam_rgb_resolution {
    XSLAM_RGB_UNDEF       = -1,
    XSLAM_RGB_1920x1080   =  0,
    XSLAM_RGB_1280x720    =  1,
    XSLAM_RGB_640x480     =  2,
    XSLAM_RGB_320x240     =  3,
    XSLAM_RGB_2560x1920   =  4,
    XSLAM_RGB_TOF          =  5
} xslam_rgb_resolution;

typedef enum xslam_rgb_source {
    XSLAM_RGB_UVC = 0,
    XSLAM_RGB_VSC = 1
} xslam_rgb_source;

/* -------------------------------------------------------------------------- */
/*  Skeleton / gesture types                                                   */
/* -------------------------------------------------------------------------- */

typedef struct xslam_hand_keypoints {
    xslam_vector3 point[21];
} xslam_hand_keypoints;

typedef struct xslam_skeleton {
    int size;
    xslam_vector3 joints_ex[52];
    xslam_vector4 pose_data[52];
    float scale[2];
    int status[2];
    double timestamp[2];
    double fisheye_timestamp;
    long long data_fetch_time_ms;
    long long data_timestamp_ms;
} xslam_skeleton;

typedef struct xslam_gesture_data {
    int index[2];
    xslam_vector3 position[2];
    xslam_vector3 slam_position[2];
    double host_timestamp;
    int64_t edge_timestamp_us;
    float distance;
    float confidence;
} xslam_gesture_data;

/* -------------------------------------------------------------------------- */
/*  Tag / QR code detection                                                    */
/* -------------------------------------------------------------------------- */

typedef struct xslam_tag_data {
    int tag_id;
    xslam_vector3 position;
    xslam_vector3 orientation;
    xslam_vector4 quaternion;
    int64_t edge_timestamp;
    double host_timestamp;
    float confidence;
    char qrcode[512];
} xslam_tag_data;

typedef struct xslam_tag_array {
    xslam_tag_data detect[64];
} xslam_tag_array;

/* -------------------------------------------------------------------------- */
/*  Surface reconstruction                                                     */
/* -------------------------------------------------------------------------- */

typedef struct xslam_surface {
    unsigned int map_id;
    unsigned int version;
    unsigned int id;
    unsigned int vertices_size;
    xslam_vector3* vertices;
    xslam_vector3* vertex_normals;
    unsigned int triangles_size;
    xslam_vector3_uint* triangles;
    xslam_vector3* texture_coordinates;
    unsigned int texture_width;
    unsigned int texture_height;
} xslam_surface;

/* -------------------------------------------------------------------------- */
/*  Slam map                                                                   */
/* -------------------------------------------------------------------------- */

typedef struct xslam_slam_map_vertex {
    xslam_vector3 vertice;
} xslam_slam_map_vertex;

/* -------------------------------------------------------------------------- */
/*  Wireless controller                                                        */
/* -------------------------------------------------------------------------- */

typedef struct xslam_controller_pos {
    int type;
    xslam_vector3 position;
    xslam_vector4 quaternion;
    float confidence;
    int key_trigger;
    int key_side;
    int rocker_x;
    int rocker_y;
    int key_a;
    int key_b;
} xslam_controller_pos;

typedef struct xslam_wireless_device_info {
    int battery;
    int temp;
} xslam_wireless_device_info;

/* -------------------------------------------------------------------------- */
/*  Event                                                                      */
/* -------------------------------------------------------------------------- */

typedef struct xslam_event {
    int type;
    int state;
    long long timestamp;
} xslam_event;

/* -------------------------------------------------------------------------- */
/*  Device status                                                              */
/* -------------------------------------------------------------------------- */

typedef struct xslam_device_status {
    int status[10];
} xslam_device_status;

/* -------------------------------------------------------------------------- */
/*  Gaze / Eye tracking                                                        */
/* -------------------------------------------------------------------------- */

typedef struct xslam_gaze_point {
    unsigned int gaze_bit_mask;
    xslam_vector3 gaze_point;
    xslam_vector3 raw_point;
    xslam_vector3 smooth_point;
    xslam_vector3 gaze_origin;
    xslam_vector3 gaze_direction;
    float re;
    unsigned int ex_data_bit_mask;
} xslam_gaze_point;

typedef struct xslam_pupil_info {
    unsigned int pupil_bit_mask;
    xslam_vector2 pupil_center;
    float pupil_distance;
    float pupil_diameter;
    float pupil_diameter_mm;
    float pupil_minor_axis;
    float pupil_minor_axis_mm;
} xslam_pupil_info;

typedef struct xslam_gaze_calib_status {
    int enter_status;
    int collect_status;
    int setup_status;
    int compute_apply_status;
    int leave_status;
    int reset_status;
} xslam_gaze_calib_status;

/* -------------------------------------------------------------------------- */
/*  GPS / BeiDou                                                               */
/* -------------------------------------------------------------------------- */

typedef struct xslam_beidou_gps_data {
    int data_ready_flag;
    double lat_data;
    int latdir;
    double lon_data;
    int londir;
    int satellite_num;
    int mode;
} xslam_beidou_gps_data;

/* -------------------------------------------------------------------------- */
/*  Callback function pointer types                                            */
/* -------------------------------------------------------------------------- */

typedef void (*xslam_cb_data)(unsigned char* data, int len);
typedef void (*xslam_fn_surface_callback)(xslam_surface* surface, int size);
typedef void (*xslam_fn_skeleton_callback)(xslam_skeleton skeleton);
typedef void (*xslam_fn_gesture_callback)(xslam_gesture_data gesture);
typedef void (*xslam_fn_event_callback)(xslam_event event);
typedef void (*xslam_fn_beidou_callback)(xslam_beidou_gps_data data);
typedef void (*xslam_fn_device_status_callback)(const unsigned char* status, int length);
typedef void (*xslam_fn_device_status_callback_ex)(xslam_device_status status);
typedef void (*xslam_fn_controller_callback)(xslam_controller_pos* pose);
typedef void (*xslam_fn_wireless_scan_callback)(const char* name, const char* mac);
typedef void (*xslam_fn_wireless_state_callback)(const char* name, const char* mac, int state);
typedef void (*xslam_fn_wireless_upload_callback)(int ret);
typedef void (*xslam_cslam_switched_callback)(int map_quality);
typedef void (*xslam_cslam_localized_callback)(float percent);
typedef void (*xslam_cslam_saved_callback)(int status, int map_quality);

#ifdef __cplusplus
}
#endif

#endif /* XSLAM_TYPES_H */
