import { useRef, useMemo, useEffect } from 'react'
import { useFrame, useThree, createPortal } from '@react-three/fiber'
import { useFBO } from '@react-three/drei'
import * as THREE from 'three'
import { useTrackingStore } from '../stores/tracking'
import { useCalibrationProfilesStore } from '../stores/calibrationProfiles'
import { xr50ToScene, xr50ToEuler } from '../lib/transforms'
import { type NorthStarCalibration } from '../lib/calibration'
import { Scene } from './Scene'
import { Hands } from './Hands'

interface Props {
  calibration: NorthStarCalibration
  outputMode?: 'panel' | 'viewer'
}

const NEAR = 0.01
const FAR = 1000

// ---- Shared vertex shader ----
const VERT = `
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position, 1.0);
}
`

// ---- Stereo composition + undistortion fragment shader ----
const COMPOSE_FRAG = `
uniform sampler2D leftEye;
uniform sampler2D rightEye;
uniform float leftCoeffsX[16];
uniform float leftCoeffsY[16];
uniform float rightCoeffsX[16];
uniform float rightCoeffsY[16];
uniform float leftUvScale;
uniform float rightUvScale;
uniform vec2 leftUvOffset;
uniform vec2 rightUvOffset;
uniform bool undistortionEnabled;
varying vec2 vUv;

vec2 evalPoly(float cx[16], float cy[16], vec2 uv) {
  float u = uv.x, v = uv.y;
  float u2 = u*u, v2 = v*v, u3 = u2*u, v3 = v2*v;

  // Northstar V2 coefficients are numpy polyvander2d row-major:
  // c00, c01*v, c02*v^2, c03*v^3, c10*u, c11*u*v, ...
  return vec2(
    cx[0]+cx[1]*v+cx[2]*v2+cx[3]*v3+cx[4]*u+cx[5]*u*v+cx[6]*u*v2+cx[7]*u*v3
    +cx[8]*u2+cx[9]*u2*v+cx[10]*u2*v2+cx[11]*u2*v3+cx[12]*u3+cx[13]*u3*v+cx[14]*u3*v2+cx[15]*u3*v3,
    cy[0]+cy[1]*v+cy[2]*v2+cy[3]*v3+cy[4]*u+cy[5]*u*v+cy[6]*u*v2+cy[7]*u*v3
    +cy[8]*u2+cy[9]*u2*v+cy[10]*u2*v2+cy[11]*u2*v3+cy[12]*u3+cy[13]*u3*v+cy[14]*u3*v2+cy[15]*u3*v3
  );
}

void main() {
  vec2 uv = vUv;
  if (uv.x < 0.5) {
    vec2 eyeUv = vec2(uv.x * 2.0, uv.y);
    if (undistortionEnabled) eyeUv = evalPoly(leftCoeffsX, leftCoeffsY, eyeUv);
    eyeUv = (eyeUv - vec2(0.5)) * leftUvScale + vec2(0.5) + leftUvOffset;
    gl_FragColor = (eyeUv.x<0.0||eyeUv.x>1.0||eyeUv.y<0.0||eyeUv.y>1.0)
      ? vec4(0.0) : texture2D(leftEye, eyeUv);
  } else {
    vec2 eyeUv = vec2((uv.x - 0.5) * 2.0, uv.y);
    if (undistortionEnabled) eyeUv = evalPoly(rightCoeffsX, rightCoeffsY, eyeUv);
    eyeUv = (eyeUv - vec2(0.5)) * rightUvScale + vec2(0.5) + rightUvOffset;
    gl_FragColor = (eyeUv.x<0.0||eyeUv.x>1.0||eyeUv.y<0.0||eyeUv.y>1.0)
      ? vec4(0.0) : texture2D(rightEye, eyeUv);
  }
}
`

// ---- Temporal re-projection fragment shader ----
const REPROJ_FRAG = `
uniform sampler2D inputTexture;
uniform mat3 homography;
uniform bool reprojectionEnabled;
varying vec2 vUv;

void main() {
  if (!reprojectionEnabled) {
    gl_FragColor = texture2D(inputTexture, vUv);
    return;
  }
  vec3 warped = homography * vec3(vUv, 1.0);
  vec2 warpedUv = warped.xy / warped.z;
  gl_FragColor = (warpedUv.x<0.0||warpedUv.x>1.0||warpedUv.y<0.0||warpedUv.y>1.0)
    ? vec4(0.0, 0.0, 0.0, 1.0) : texture2D(inputTexture, warpedUv);
}
`

/**
 * Compute a 2D homography from rotation delta between render and display poses.
 * Simplified planar assumption: H ≈ R_delta projected to 2D.
 */
const _rotMat = new THREE.Matrix4()
const _deltaQuat = new THREE.Quaternion()
const identityPolyX = Array.from({ length: 16 }, (_, i) => (i === 4 ? 1 : 0))
const identityPolyY = Array.from({ length: 16 }, (_, i) => (i === 1 ? 1 : 0))

const computeHomography = (
  renderQuat: THREE.Quaternion,
  displayQuat: THREE.Quaternion,
  out: THREE.Matrix3,
): THREE.Matrix3 => {
  _deltaQuat.copy(displayQuat).multiply(renderQuat.clone().invert())
  _rotMat.makeRotationFromQuaternion(_deltaQuat)
  const e = _rotMat.elements
  // Extract 3x3 rotation → UV space homography
  out.set(e[0], e[4], e[12], e[1], e[5], e[13], e[2], e[6], e[10])
  return out
}

export function NorthStarRenderer({ calibration, outputMode = 'panel' }: Props) {
  const { gl } = useThree()
  const cameraRigRef = useRef<THREE.Group>(null)
  const undistortionRef = useRef(true)
  const reprojectionRef = useRef(true)
  const mirroredRef = useRef(true)
  const renderQuatRef = useRef(new THREE.Quaternion())
  const homographyRef = useRef(new THREE.Matrix3())

  // Virtual scene
  const virtualScene = useMemo(() => {
    const s = new THREE.Scene()
    s.background = new THREE.Color(0x111122)
    return s
  }, [])

  // Eye cameras
  const leftCamera = useMemo(() => {
    const cam = new THREE.PerspectiveCamera()
    const { frustum } = calibration.leftEye
    cam.projectionMatrix.makePerspective(
      frustum.left * NEAR, frustum.right * NEAR,
      frustum.top * NEAR, frustum.bottom * NEAR, NEAR, FAR,
    )
    cam.projectionMatrixInverse.copy(cam.projectionMatrix).invert()
    return cam
  }, [calibration])

  const rightCamera = useMemo(() => {
    const cam = new THREE.PerspectiveCamera()
    const { frustum } = calibration.rightEye
    cam.projectionMatrix.makePerspective(
      frustum.left * NEAR, frustum.right * NEAR,
      frustum.top * NEAR, frustum.bottom * NEAR, NEAR, FAR,
    )
    cam.projectionMatrixInverse.copy(cam.projectionMatrix).invert()
    return cam
  }, [calibration])

  // FBOs
  const eyeW = calibration.display.eyeWidth
  const eyeH = calibration.display.eyeHeight
  const leftFBO = useFBO(eyeW, eyeH, { stencilBuffer: false })
  const rightFBO = useFBO(eyeW, eyeH, { stencilBuffer: false })
  const composedFBO = useFBO(calibration.display.width, calibration.display.height, { stencilBuffer: false })

  // Composition material
  const composeMaterial = useMemo(() => {
    const arr = (a: Float32Array) => Array.from(a)
    return new THREE.ShaderMaterial({
      uniforms: {
        leftEye: { value: leftFBO.texture },
        rightEye: { value: rightFBO.texture },
        leftCoeffsX: { value: arr(calibration.leftEye.uvToRectX) },
        leftCoeffsY: { value: arr(calibration.leftEye.uvToRectY) },
        rightCoeffsX: { value: arr(calibration.rightEye.uvToRectX) },
        rightCoeffsY: { value: arr(calibration.rightEye.uvToRectY) },
        leftUvScale: { value: 1.0 },
        rightUvScale: { value: 1.0 },
        leftUvOffset: { value: new THREE.Vector2(0, 0) },
        rightUvOffset: { value: new THREE.Vector2(0, 0) },
        undistortionEnabled: { value: true },
      },
      vertexShader: VERT, fragmentShader: COMPOSE_FRAG,
      depthTest: false, depthWrite: false,
    })
  }, [leftFBO.texture, rightFBO.texture, calibration])

  // Re-projection material
  const reprojMaterial = useMemo(() =>
    new THREE.ShaderMaterial({
      uniforms: {
        inputTexture: { value: composedFBO.texture },
        homography: { value: new THREE.Matrix3() },
        reprojectionEnabled: { value: true },
      },
      vertexShader: VERT, fragmentShader: REPROJ_FRAG,
      depthTest: false, depthWrite: false,
    }),
  [composedFBO.texture])

  // Scenes for fullscreen passes
  const orthoCamera = useMemo(() => new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1), [])
  const composeScene = useMemo(() => new THREE.Scene(), [])
  const reprojScene = useMemo(() => new THREE.Scene(), [])

  useEffect(() => {
    const geo = new THREE.PlaneGeometry(2, 2)
    const cq = new THREE.Mesh(geo, composeMaterial)
    const rq = new THREE.Mesh(geo.clone(), reprojMaterial)
    composeScene.add(cq)
    reprojScene.add(rq)
    return () => {
      composeScene.remove(cq)
      reprojScene.remove(rq)
      geo.dispose()
    }
  }, [composeScene, reprojScene, composeMaterial, reprojMaterial])

  // Keyboard controls
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'u' || e.key === 'U') {
        undistortionRef.current = !undistortionRef.current
        composeMaterial.uniforms.undistortionEnabled.value = undistortionRef.current
        console.log(`[NorthStar] undistortion: ${undistortionRef.current ? 'ON' : 'OFF'}`)
      }
      if (e.key === 'r' || e.key === 'R') {
        reprojectionRef.current = !reprojectionRef.current
        reprojMaterial.uniforms.reprojectionEnabled.value = reprojectionRef.current
        console.log(`[NorthStar] re-projection: ${reprojectionRef.current ? 'ON' : 'OFF'}`)
      }
      if (e.key === 'm' || e.key === 'M') {
        mirroredRef.current = !mirroredRef.current
        gl.domElement.style.transform = mirroredRef.current ? 'scaleX(-1)' : 'none'
        console.log(`[NorthStar] mirror: ${mirroredRef.current ? 'ON' : 'OFF'}`)
      }
    }
    window.addEventListener('keydown', onKey)
    gl.domElement.style.transform = outputMode === 'panel' ? 'scaleX(-1)' : 'none'
    return () => window.removeEventListener('keydown', onKey)
  }, [gl, composeMaterial, reprojMaterial, outputMode])

  // ---- Main render loop ----
  useFrame(() => {
    if (!cameraRigRef.current) return
    const profile = useCalibrationProfilesStore.getState().settings
    const { headPose } = useTrackingStore.getState()
    const pos = xr50ToScene(headPose.x, headPose.y, headPose.z)
    const rot = xr50ToEuler(headPose.roll, headPose.pitch, headPose.yaw)

    const blend = (base: number[], identity: number[], strength: number) =>
      base.map((v, i) => identity[i] + (v - identity[i]) * strength)

    const panelOutput = outputMode === 'panel'
    composeMaterial.uniforms.undistortionEnabled.value = panelOutput && undistortionRef.current
    composeMaterial.uniforms.leftCoeffsX.value = panelOutput
      ? blend(Array.from(calibration.leftEye.uvToRectX), identityPolyX, profile.distortionStrength)
      : identityPolyX
    composeMaterial.uniforms.leftCoeffsY.value = panelOutput
      ? blend(Array.from(calibration.leftEye.uvToRectY), identityPolyY, profile.distortionStrength)
      : identityPolyY
    composeMaterial.uniforms.rightCoeffsX.value = panelOutput
      ? blend(Array.from(calibration.rightEye.uvToRectX), identityPolyX, profile.distortionStrength)
      : identityPolyX
    composeMaterial.uniforms.rightCoeffsY.value = panelOutput
      ? blend(Array.from(calibration.rightEye.uvToRectY), identityPolyY, profile.distortionStrength)
      : identityPolyY
    composeMaterial.uniforms.leftUvScale.value = panelOutput ? profile.leftUvScale : 1
    composeMaterial.uniforms.rightUvScale.value = panelOutput ? profile.rightUvScale : 1
    composeMaterial.uniforms.leftUvOffset.value.set(
      panelOutput ? profile.leftUvOffsetX : 0,
      panelOutput ? profile.leftUvOffsetY : 0,
    )
    composeMaterial.uniforms.rightUvOffset.value.set(
      panelOutput ? profile.rightUvOffsetX : 0,
      panelOutput ? profile.rightUvOffsetY : 0,
    )

    cameraRigRef.current.position.copy(pos)
    cameraRigRef.current.rotation.copy(rot)
    cameraRigRef.current.updateMatrixWorld(true)

    // Save render-time quaternion
    renderQuatRef.current.copy(cameraRigRef.current.quaternion)

    // Position eye cameras in world space
    const rigMatrix = cameraRigRef.current.matrixWorld

    leftCamera.position.copy(calibration.leftEye.eyePosition)
    leftCamera.position.applyMatrix4(rigMatrix)
    leftCamera.quaternion.copy(cameraRigRef.current.quaternion)

    rightCamera.position.copy(calibration.rightEye.eyePosition)
    rightCamera.position.applyMatrix4(rigMatrix)
    rightCamera.quaternion.copy(cameraRigRef.current.quaternion)

    // Pass 1: Render eyes → FBOs
    gl.setRenderTarget(leftFBO)
    gl.clear()
    gl.render(virtualScene, leftCamera)

    gl.setRenderTarget(rightFBO)
    gl.clear()
    gl.render(virtualScene, rightCamera)

    // Pass 2: Compose with undistortion → composed FBO
    gl.setRenderTarget(composedFBO)
    gl.clear()
    gl.render(composeScene, orthoCamera)

    // Pass 3: Temporal re-projection → screen
    // Get latest pose (may have updated during rendering)
    const latestPose = useTrackingStore.getState().headPose
    const displayRot = xr50ToEuler(latestPose.roll, latestPose.pitch, latestPose.yaw)
    const displayQuat = new THREE.Quaternion().setFromEuler(displayRot)

    computeHomography(renderQuatRef.current, displayQuat, homographyRef.current)
    reprojMaterial.uniforms.homography.value.copy(homographyRef.current)

    gl.setRenderTarget(null)
    gl.clear()
    gl.render(reprojScene, orthoCamera)
  }, 1)

  return (
    <>
      {createPortal(
        <>
          <Scene />
          <group ref={cameraRigRef}>
            <Hands />
          </group>
        </>,
        virtualScene,
      )}
    </>
  )
}
