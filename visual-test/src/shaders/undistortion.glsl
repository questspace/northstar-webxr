/**
 * NorthStar Undistortion + Stereo Composition Shader
 *
 * Evaluates the v2 polynomial UV warp (16 coefficients per axis per eye)
 * to pre-correct for the NorthStar combiner optics, then samples each
 * eye texture at the warped UV coordinates.
 *
 * Polynomial form (4th order with cross-terms):
 *   f(u,v) = c[0]  + c[1]*u  + c[2]*v  + c[3]*u*v
 *          + c[4]*u² + c[5]*v² + c[6]*u²*v + c[7]*u*v²
 *          + c[8]*u³ + c[9]*v³ + c[10]*u²*v² + c[11]*u³*v
 *          + c[12]*u*v³ + c[13]*u³*v² + c[14]*u²*v³ + c[15]*u³*v³
 */

uniform sampler2D leftEye;
uniform sampler2D rightEye;

// 16 polynomial coefficients for each axis of each eye
uniform float leftCoeffsX[16];
uniform float leftCoeffsY[16];
uniform float rightCoeffsX[16];
uniform float rightCoeffsY[16];

uniform bool undistortionEnabled;

varying vec2 vUv;

vec2 evaluatePolynomial(float coeffsX[16], float coeffsY[16], vec2 uv) {
  float u = uv.x;
  float v = uv.y;

  float u2 = u * u;
  float v2 = v * v;
  float u3 = u2 * u;
  float v3 = v2 * v;
  float uv1 = u * v;
  float u2v = u2 * v;
  float uv2 = u * v2;
  float u2v2 = u2 * v2;
  float u3v = u3 * v;
  float uv3 = u * v3;
  float u3v2 = u3 * v2;
  float u2v3 = u2 * v3;
  float u3v3 = u3 * v3;

  float rx = coeffsX[0]  + coeffsX[1]*u  + coeffsX[2]*v  + coeffsX[3]*uv1
           + coeffsX[4]*u2 + coeffsX[5]*v2 + coeffsX[6]*u2v + coeffsX[7]*uv2
           + coeffsX[8]*u3 + coeffsX[9]*v3 + coeffsX[10]*u2v2 + coeffsX[11]*u3v
           + coeffsX[12]*uv3 + coeffsX[13]*u3v2 + coeffsX[14]*u2v3 + coeffsX[15]*u3v3;

  float ry = coeffsY[0]  + coeffsY[1]*u  + coeffsY[2]*v  + coeffsY[3]*uv1
           + coeffsY[4]*u2 + coeffsY[5]*v2 + coeffsY[6]*u2v + coeffsY[7]*uv2
           + coeffsY[8]*u3 + coeffsY[9]*v3 + coeffsY[10]*u2v2 + coeffsY[11]*u3v
           + coeffsY[12]*uv3 + coeffsY[13]*u3v2 + coeffsY[14]*u2v3 + coeffsY[15]*u3v3;

  return vec2(rx, ry);
}

void main() {
  vec2 uv = vUv;

  if (uv.x < 0.5) {
    // Left eye
    vec2 eyeUv = vec2(uv.x * 2.0, uv.y);
    if (undistortionEnabled) {
      eyeUv = evaluatePolynomial(leftCoeffsX, leftCoeffsY, eyeUv);
    }
    if (eyeUv.x < 0.0 || eyeUv.x > 1.0 || eyeUv.y < 0.0 || eyeUv.y > 1.0) {
      gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
    } else {
      gl_FragColor = texture2D(leftEye, eyeUv);
    }
  } else {
    // Right eye
    vec2 eyeUv = vec2((uv.x - 0.5) * 2.0, uv.y);
    if (undistortionEnabled) {
      eyeUv = evaluatePolynomial(rightCoeffsX, rightCoeffsY, eyeUv);
    }
    if (eyeUv.x < 0.0 || eyeUv.x > 1.0 || eyeUv.y < 0.0 || eyeUv.y > 1.0) {
      gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
    } else {
      gl_FragColor = texture2D(rightEye, eyeUv);
    }
  }
}
