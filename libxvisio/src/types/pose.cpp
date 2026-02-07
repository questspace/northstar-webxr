//
// Created by Mihir Patil on 11/5/23.
//

#include "pose.h"
#include <cmath>

namespace xv {
    Matrix3 Pose::quaternionToMatrix(const Vector4& q) {
        double w = q[0], x = q[1], y = q[2], z = q[3];
        return {{
            {1.0 - 2.0*(y*y + z*z), 2.0*(x*y - w*z),       2.0*(x*z + w*y)},
            {2.0*(x*y + w*z),       1.0 - 2.0*(x*x + z*z), 2.0*(y*z - w*x)},
            {2.0*(x*z - w*y),       2.0*(y*z + w*x),       1.0 - 2.0*(x*x + y*y)}
        }};
    }

    Vector4 Pose::matrixToQuaternion(const Matrix3&matrix) {
        Vector4 quaternion;

        double trace = matrix[0][0] + matrix[1][1] + matrix[2][2];

        if (trace > 0) {
            const double scale = sqrt(trace + 1.0) * 2.0;
            quaternion[0] = 0.25 * scale;
            quaternion[1] = (matrix[2][1] - matrix[1][2]) / scale;
            quaternion[2] = (matrix[0][2] - matrix[2][0]) / scale;
            quaternion[3] = (matrix[1][0] - matrix[0][1]) / scale;
        }
        else if (matrix[0][0] > matrix[1][1] && matrix[0][0] > matrix[2][2]) {
            const double scale = sqrt(1.0 + matrix[0][0] - matrix[1][1] - matrix[2][2]) * 2.0;
            quaternion[0] = (matrix[2][1] - matrix[1][2]) / scale;
            quaternion[1] = 0.25 * scale;
            quaternion[2] = (matrix[0][1] + matrix[1][0]) / scale;
            quaternion[3] = (matrix[0][2] + matrix[2][0]) / scale;
        }
        else if (matrix[1][1] > matrix[2][2]) {
            const double scale = sqrt(1.0 + matrix[1][1] - matrix[0][0] - matrix[2][2]) * 2.0;
            quaternion[0] = (matrix[0][2] - matrix[2][0]) / scale;
            quaternion[1] = (matrix[0][1] + matrix[1][0]) / scale;
            quaternion[2] = 0.25 * scale;
            quaternion[3] = (matrix[1][2] + matrix[2][1]) / scale;
        }
        else {
            const double scale = sqrt(1.0 + matrix[2][2] - matrix[0][0] - matrix[1][1]) * 2.0;
            quaternion[0] = (matrix[1][0] - matrix[0][1]) / scale;
            quaternion[1] = (matrix[0][2] + matrix[2][0]) / scale;
            quaternion[2] = (matrix[1][2] + matrix[2][1]) / scale;
            quaternion[3] = 0.25 * scale;
        }
        return quaternion;
    }
}
