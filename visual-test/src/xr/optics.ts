import * as THREE from 'three'
import type { NorthStarCalibration } from '../lib/calibration'
import type { CalibrationProfileSettings } from '../lib/calibrationProfiles'

/**
 * NorthStar V2 optics, matching Project Esky's native renderer:
 *  - each eye is a plain perspective view, 70° FOV across the panel's short
 *    (1440 px) axis, rendered into one half of the side-by-side 2880x1600 image;
 *  - the calibration polynomials map a panel pixel (u, v with v measured from
 *    the top row, as the gray-code fit sampled it) to a rectilinear ray:
 *    rect.x runs along the panel's vertical axis, rect.y along its horizontal
 *    axis (seen in the coefficient field and the V1 screen matrices); the ray
 *    is projected through the eye camera to find the source texel.
 * The sign of each ray axis is a profile setting (rectSignX/Y) so a flipped
 * view can be corrected while wearing the headset; the verified defaults are
 * X = +1, Y = +1 (the panel's top row shows the bottom of the view, which the
 * combiner reflection turns the right way up).
 */
export const EYE_FOV_DEG = 70

/** Eye image size: one panel half, portrait. */
export const eyeSize = (c: NorthStarCalibration) => ({
  width: c.display.width / 2,
  height: c.display.height,
})

/** Half-FOV tangents (horizontal, vertical) of the eye camera. */
export const eyeTangents = (c: NorthStarCalibration) => {
  const { width, height } = eyeSize(c)
  const h = Math.tan(THREE.MathUtils.degToRad(EYE_FOV_DEG / 2))
  return { h, v: (h * height) / width }
}

export const eyeProjection = (
  c: NorthStarCalibration,
  near: number,
  far: number,
  out = new THREE.Matrix4(),
) => {
  const { h, v } = eyeTangents(c)
  return out.makePerspective(-h * near, h * near, v * near, -v * near, near, far)
}

/** Camera-space ray rotation taking the displayed pose back to the rendered one. */
export const reprojection = (render: THREE.Quaternion, display: THREE.Quaternion) =>
  new THREE.Matrix3().setFromMatrix4(
    new THREE.Matrix4().makeRotationFromQuaternion(render.clone().invert().multiply(display)),
  )

export interface WarpParams {
  strength: number
  rectSign: [number, number]
  offsets: { left: [number, number]; right: [number, number] }
  reproject: THREE.Matrix3
}

export const warpParams = (
  s: CalibrationProfileSettings,
  strength: number,
  reproject = new THREE.Matrix3(),
): WarpParams => ({
  strength,
  rectSign: [s.rectSignX, s.rectSignY],
  offsets: { left: [s.leftUvOffsetX, s.leftUvOffsetY], right: [s.rightUvOffsetX, s.rightUvOffsetY] },
  reproject,
})

const VERT = `
attribute vec2 position;
varying vec2 vUv;
void main() {
  vUv = position * 0.5 + 0.5;
  gl_Position = vec4(position, 0.0, 1.0);
}
`

const FRAG = `
precision highp float;
uniform sampler2D source;
uniform float leftX[16];
uniform float leftY[16];
uniform float rightX[16];
uniform float rightY[16];
uniform vec2 rectSign;
uniform vec2 tanHalf;
uniform float strength;
uniform vec2 offsetLeft;
uniform vec2 offsetRight;
uniform mat3 reproject;
varying vec2 vUv;

// numpy polyvander2d row-major: c[i*4+j] * X^i * Y^j
float poly(float c[16], float X, float Y) {
  float X2 = X * X, X3 = X2 * X, Y2 = Y * Y, Y3 = Y2 * Y;
  return c[0] + c[1] * Y + c[2] * Y2 + c[3] * Y3
    + (c[4] + c[5] * Y + c[6] * Y2 + c[7] * Y3) * X
    + (c[8] + c[9] * Y + c[10] * Y2 + c[11] * Y3) * X2
    + (c[12] + c[13] * Y + c[14] * Y2 + c[15] * Y3) * X3;
}

vec2 warp(float cx[16], float cy[16], vec2 local, vec2 offset) {
  vec2 rect = vec2(poly(cx, local.x, 1.0 - local.y), poly(cy, local.x, 1.0 - local.y));
  vec3 ray = reproject * vec3(rectSign.x * rect.y, rectSign.y * rect.x, -1.0);
  vec2 eyeUv = 0.5 + 0.5 * ray.xy / (-ray.z * tanHalf);
  return mix(local, eyeUv, strength) + offset;
}

void main() {
  bool left = vUv.x < 0.5;
  vec2 local = vec2(left ? vUv.x * 2.0 : vUv.x * 2.0 - 1.0, vUv.y);
  vec2 uv = left ? warp(leftX, leftY, local, offsetLeft) : warp(rightX, rightY, local, offsetRight);
  bool outside = uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0;
  vec2 src = vec2(left ? uv.x * 0.5 : 0.5 + uv.x * 0.5, uv.y);
  gl_FragColor = outside ? vec4(0.0, 0.0, 0.0, 1.0) : texture2D(source, src);
}
`

type GL = WebGLRenderingContext | WebGL2RenderingContext

const compile = (gl: GL, type: number, source: string) => {
  const shader = gl.createShader(type)!
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(`[optics] shader: ${gl.getShaderInfoLog(shader)}`)
  }
  return shader
}

/**
 * Fullscreen warp pass in raw WebGL so it can run on any app's context (three.js,
 * Babylon, ...) after the app has drawn both eyes side by side. Every piece of GL
 * state it touches is restored, so renderer state caches stay valid.
 */
export const createWarpPass = (gl: GL, calibration: NorthStarCalibration) => {
  const gl2 = gl instanceof WebGL2RenderingContext ? gl : null
  const program = gl.createProgram()!
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERT))
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAG))
  gl.bindAttribLocation(program, 0, 'position')
  gl.linkProgram(program)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`[optics] link: ${gl.getProgramInfoLog(program)}`)
  }
  const uniform = (name: string) => gl.getUniformLocation(program, name)

  const quad = gl.createBuffer()!
  const previousBuffer = gl.getParameter(gl.ARRAY_BUFFER_BINDING)
  gl.bindBuffer(gl.ARRAY_BUFFER, quad)
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
  gl.bindBuffer(gl.ARRAY_BUFFER, previousBuffer)

  const previousProgram = gl.getParameter(gl.CURRENT_PROGRAM)
  gl.useProgram(program)
  gl.uniform1fv(uniform('leftX'), calibration.leftEye.uvToRectX)
  gl.uniform1fv(uniform('leftY'), calibration.leftEye.uvToRectY)
  gl.uniform1fv(uniform('rightX'), calibration.rightEye.uvToRectX)
  gl.uniform1fv(uniform('rightY'), calibration.rightEye.uvToRectY)
  const { h, v } = eyeTangents(calibration)
  gl.uniform2f(uniform('tanHalf'), h, v)
  gl.uniform1i(uniform('source'), 0)
  gl.useProgram(previousProgram)

  const uStrength = uniform('strength')
  const uRectSign = uniform('rectSign')
  const uOffsetLeft = uniform('offsetLeft')
  const uOffsetRight = uniform('offsetRight')
  const uReproject = uniform('reproject')

  const capture = gl.createTexture()!
  let captureWidth = 0
  let captureHeight = 0

  /** Draws `texture` (side-by-side eyes) warped onto the whole drawing buffer. */
  const draw = (texture: WebGLTexture, params: WarpParams) => {
    const saved = {
      program: gl.getParameter(gl.CURRENT_PROGRAM),
      buffer: gl.getParameter(gl.ARRAY_BUFFER_BINDING),
      vao: gl2?.getParameter(gl2.VERTEX_ARRAY_BINDING),
      activeTexture: gl.getParameter(gl.ACTIVE_TEXTURE),
      framebuffer: gl.getParameter(gl.FRAMEBUFFER_BINDING),
      viewport: gl.getParameter(gl.VIEWPORT) as Int32Array,
      colorMask: gl.getParameter(gl.COLOR_WRITEMASK) as boolean[],
      enabled: [gl.DEPTH_TEST, gl.BLEND, gl.CULL_FACE, gl.SCISSOR_TEST, gl.STENCIL_TEST].filter((cap) => gl.isEnabled(cap)),
    }
    gl2?.bindVertexArray(null)
    const attribEnabled = gl.getVertexAttrib(0, gl.VERTEX_ATTRIB_ARRAY_ENABLED)
    gl.activeTexture(gl.TEXTURE0)
    const savedTexture = gl.getParameter(gl.TEXTURE_BINDING_2D)

    gl.bindFramebuffer(gl.FRAMEBUFFER, null)
    gl.useProgram(program)
    gl.uniform1f(uStrength, params.strength)
    gl.uniform2f(uRectSign, params.rectSign[0], params.rectSign[1])
    gl.uniform2f(uOffsetLeft, params.offsets.left[0], params.offsets.left[1])
    gl.uniform2f(uOffsetRight, params.offsets.right[0], params.offsets.right[1])
    gl.uniformMatrix3fv(uReproject, false, params.reproject.elements)
    gl.bindTexture(gl.TEXTURE_2D, texture)
    gl.bindBuffer(gl.ARRAY_BUFFER, quad)
    gl.enableVertexAttribArray(0)
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0)
    gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight)
    saved.enabled.forEach((cap) => gl.disable(cap))
    gl.colorMask(true, true, true, true)
    gl.drawArrays(gl.TRIANGLES, 0, 3)

    if (!attribEnabled) gl.disableVertexAttribArray(0)
    gl.bindTexture(gl.TEXTURE_2D, savedTexture)
    gl.activeTexture(saved.activeTexture)
    gl.bindBuffer(gl.ARRAY_BUFFER, saved.buffer)
    gl2?.bindVertexArray(saved.vao)
    gl.useProgram(saved.program)
    gl.bindFramebuffer(gl.FRAMEBUFFER, saved.framebuffer)
    gl.viewport(saved.viewport[0], saved.viewport[1], saved.viewport[2], saved.viewport[3])
    gl.colorMask(saved.colorMask[0], saved.colorMask[1], saved.colorMask[2], saved.colorMask[3])
    saved.enabled.forEach((cap) => gl.enable(cap))
  }

  /** Warps the current drawing buffer in place (copies it to a texture first). */
  const warpCanvas = (params: WarpParams) => {
    const width = gl.drawingBufferWidth
    const height = gl.drawingBufferHeight
    const activeTexture = gl.getParameter(gl.ACTIVE_TEXTURE)
    const framebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING)
    gl.activeTexture(gl.TEXTURE0)
    const savedTexture = gl.getParameter(gl.TEXTURE_BINDING_2D)
    gl.bindFramebuffer(gl.FRAMEBUFFER, null)
    gl.bindTexture(gl.TEXTURE_2D, capture)
    if (width !== captureWidth || height !== captureHeight) {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null)
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR)
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR)
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)
      captureWidth = width
      captureHeight = height
    }
    gl.copyTexSubImage2D(gl.TEXTURE_2D, 0, 0, 0, 0, 0, width, height)
    gl.bindTexture(gl.TEXTURE_2D, savedTexture)
    gl.activeTexture(activeTexture)
    gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer)
    draw(capture, params)
  }

  return { draw, warpCanvas }
}

export type WarpPass = ReturnType<typeof createWarpPass>
