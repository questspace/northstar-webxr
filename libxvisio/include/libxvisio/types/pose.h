/**
 * @file pose.h
 * @brief 6DOF pose data structure
 */

#ifndef XVISIO_POSE_H
#define XVISIO_POSE_H

#include <array>

namespace xv {

using Matrix3 = std::array<std::array<double, 3>, 3>;
using Vector3 = std::array<double, 3>;
using Vector4 = std::array<double, 4>;

struct Pose {
    Pose(const Vector3& pos, const Matrix3& rot, int64_t ts)
        : position(pos)
        , matrix(rot)
        , quaternion(matrixToQuaternion(rot))
        , timestamp(ts) {}

    static Vector4 matrixToQuaternion(const Matrix3& matrix);

    Vector3 position;    ///< Position in meters (X, Y, Z)
    Matrix3 matrix;      ///< Rotation matrix
    Vector4 quaternion;  ///< Rotation quaternion (W, X, Y, Z)
    int64_t timestamp;   ///< Timestamp in microseconds
};

} // namespace xv

#endif // XVISIO_POSE_H
