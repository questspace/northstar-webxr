/**
 * Planar Temporal Re-projection Shader
 *
 * Based on the simplified approach from the Project Esky paper:
 * assumes content lies on the display's focal plane.
 *
 * Given the rotation delta between render-time and display-time poses,
 * applies a 2D homography warp: H = K * R_delta * K^-1
 *
 * This compensates for head rotation that occurred between
 * when the frame was rendered and when it's displayed.
 */

uniform sampler2D inputTexture;
uniform mat3 homography;
uniform bool reprojectionEnabled;
varying vec2 vUv;

void main() {
  if (!reprojectionEnabled) {
    gl_FragColor = texture2D(inputTexture, vUv);
    return;
  }

  // Apply homography: map display-time UV → render-time UV
  vec3 warped = homography * vec3(vUv, 1.0);
  vec2 warpedUv = warped.xy / warped.z;

  // Black outside bounds
  if (warpedUv.x < 0.0 || warpedUv.x > 1.0 || warpedUv.y < 0.0 || warpedUv.y > 1.0) {
    gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
  } else {
    gl_FragColor = texture2D(inputTexture, warpedUv);
  }
}
