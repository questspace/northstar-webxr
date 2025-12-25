/**
 * Project NorthStar Stereo VR Renderer
 * Renders side-by-side stereo with lens distortion for NorthStar headset.
 * Uses XR50 for 6DOF tracking and Ultraleap for hands.
 */

import * as THREE from 'three';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js';

// ============================================================================
// CONFIGURATION
// ============================================================================

const CONFIG = {
    // NorthStar display
    width: 2880,
    height: 1600,
    eyeWidth: 1440,  // Per eye
    ipd: 0.064,      // Inter-pupillary distance in meters
    
    // Lens distortion (adjust these for your specific NorthStar build)
    distortion: {
        k1: 0.0,     // Radial distortion coefficient
        k2: 0.0,     // Radial distortion coefficient  
        enabled: false  // Enable when calibrated
    },
    
    // XR50 position scale (XR50 outputs small values, scale up)
    positionScale: 2.0
};

// ============================================================================
// SCENE SETUP
// ============================================================================

const scene = new THREE.Scene();
scene.background = new THREE.Color(0x111122);

// Two cameras for stereo
const cameraLeft = new THREE.PerspectiveCamera(90, CONFIG.eyeWidth / CONFIG.height, 0.01, 1000);
const cameraRight = new THREE.PerspectiveCamera(90, CONFIG.eyeWidth / CONFIG.height, 0.01, 1000);

// Camera rig (moves with head tracking)
const cameraRig = new THREE.Group();
cameraRig.add(cameraLeft);
cameraRig.add(cameraRight);
cameraLeft.position.x = -CONFIG.ipd / 2;
cameraRight.position.x = CONFIG.ipd / 2;
scene.add(cameraRig);

// Renderer
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setSize(CONFIG.width, CONFIG.height);
renderer.setPixelRatio(1);
renderer.autoClear = false;
document.body.appendChild(renderer.domElement);

// Render targets for each eye
const renderTargetLeft = new THREE.WebGLRenderTarget(CONFIG.eyeWidth, CONFIG.height);
const renderTargetRight = new THREE.WebGLRenderTarget(CONFIG.eyeWidth, CONFIG.height);

// ============================================================================
// ENVIRONMENT
// ============================================================================

// Lighting
scene.add(new THREE.AmbientLight(0xffffff, 0.4));
const sunLight = new THREE.DirectionalLight(0xffffff, 0.8);
sunLight.position.set(5, 10, 5);
scene.add(sunLight);

// Ground grid
const grid = new THREE.GridHelper(20, 40, 0x444466, 0x333355);
scene.add(grid);

// Floating cubes for depth reference
const cubeGeo = new THREE.BoxGeometry(0.1, 0.1, 0.1);
const cubeMat = new THREE.MeshPhongMaterial({ color: 0x00ffff, emissive: 0x004444 });
for (let i = 0; i < 20; i++) {
    const cube = new THREE.Mesh(cubeGeo, cubeMat.clone());
    cube.position.set(
        (Math.random() - 0.5) * 4,
        Math.random() * 2 + 0.5,
        (Math.random() - 0.5) * 4
    );
    cube.userData.rotSpeed = Math.random() * 0.02;
    scene.add(cube);
}

// ============================================================================
// HANDS
// ============================================================================

const jointMat = new THREE.MeshPhongMaterial({ color: 0x00ff88, emissive: 0x00ff88, emissiveIntensity: 0.5 });
const palmMat = new THREE.MeshPhongMaterial({ color: 0xff6600, emissive: 0xff6600, emissiveIntensity: 0.5 });
const jointGeo = new THREE.SphereGeometry(0.006, 8, 8);   // 6mm joints
const palmGeo = new THREE.SphereGeometry(0.012, 12, 12);  // 12mm palm

const hands = { left: createHand(), right: createHand() };
// Hands are children of camera rig (relative to head position)
cameraRig.add(hands.left);
cameraRig.add(hands.right);

function createHand() {
    const hand = new THREE.Group();
    const palm = new THREE.Mesh(palmGeo, palmMat.clone());
    palm.name = 'palm';
    hand.add(palm);
    
    const joints = [];
    for (let i = 0; i < 20; i++) {
        const joint = new THREE.Mesh(jointGeo, jointMat.clone());
        joints.push(joint);
        hand.add(joint);
    }
    hand.userData.joints = joints;
    hand.visible = false;
    return hand;
}

// Convert Leap mm to head-relative position
// Leap on NorthStar points downward, so axes are rotated:
// Leap Y (up from device) = toward face in head space
// Leap Z (away from device) = down in head space
function leapToScene(pos) {
    const scale = 0.001; // mm to m
    const x = -(pos[0] ?? 0) * scale;           // Mirror X for correct handedness
    const y = -(pos[2] ?? 0) * scale + 0.1;     // Leap Z → head Y (with offset)
    const z = -(pos[1] ?? 0) * scale + 0.3;     // Leap Y → head -Z (in front of face)
    return new THREE.Vector3(x, y, z);
}

function updateHand(handData, handObj, pointables) {
    if (!handData) {
        handObj.visible = false;
        return;
    }
    handObj.visible = true;
    
    const palm = handObj.getObjectByName('palm');
    if (palm && handData.palmPosition) {
        palm.position.copy(leapToScene(handData.palmPosition));
    }
    
    const fingers = pointables?.filter(p => p.handId === handData.id) || [];
    fingers.sort((a, b) => (a.type || 0) - (b.type || 0));
    
    let idx = 0;
    for (const f of fingers) {
        for (const pos of [f.mcpPosition, f.pipPosition, f.dipPosition, f.tipPosition]) {
            if (pos && idx < 20) {
                handObj.userData.joints[idx].position.copy(leapToScene(pos));
                handObj.userData.joints[idx].visible = true;
                idx++;
            }
        }
    }
    for (let i = idx; i < 20; i++) handObj.userData.joints[i].visible = false;
}

// ============================================================================
// WEBSOCKET CONNECTIONS
// ============================================================================

const state = {
    xr50Connected: false,
    leapConnected: false,
    headPos: new THREE.Vector3(0, 1.6, 0),  // Default eye height
    headRot: new THREE.Euler(0, 0, 0, 'YXZ'),
    fps: 0,
    frameCount: 0,
    lastFpsTime: Date.now()
};

// XR50 WebSocket
const xr50Ws = new WebSocket('ws://localhost:8081');
xr50Ws.onopen = () => {
    state.xr50Connected = true;
    document.getElementById('xr50-dot').classList.add('on');
};
xr50Ws.onmessage = (e) => {
    try {
        const d = JSON.parse(e.data);
        state.headPos.set(
            d.x * CONFIG.positionScale,
            d.y * CONFIG.positionScale + 1.6,
            -d.z * CONFIG.positionScale
        );
        state.headRot.set(
            THREE.MathUtils.degToRad(d.pitch),
            THREE.MathUtils.degToRad(d.yaw),
            THREE.MathUtils.degToRad(d.roll),
            'YXZ'
        );
        state.frameCount++;
    } catch (err) {}
};
xr50Ws.onclose = () => {
    state.xr50Connected = false;
    document.getElementById('xr50-dot').classList.remove('on');
};

// Ultraleap WebSocket
const leapWs = new WebSocket('ws://localhost:6437/v6.json');
let leapDebugCount = 0;

leapWs.onopen = () => {
    state.leapConnected = true;
    document.getElementById('leap-dot').classList.add('on');
    leapWs.send(JSON.stringify({ enableGestures: true }));
    console.log('Ultraleap connected');
};
leapWs.onmessage = (e) => {
    try {
        const frame = JSON.parse(e.data);
        if (!frame.hands) return;
        
        // Debug first few frames
        if (leapDebugCount < 3 && frame.hands.length > 0) {
            console.log('Leap frame:', frame.hands.length, 'hands');
            console.log('Hand 0 palm:', frame.hands[0].palmPosition);
            console.log('Pointables:', frame.pointables?.length);
            leapDebugCount++;
        }
        
        let left = null, right = null;
        for (const h of frame.hands) {
            if (h.type === 'left') left = h;
            else if (h.type === 'right') right = h;
        }
        updateHand(left, hands.left, frame.pointables);
        updateHand(right, hands.right, frame.pointables);
    } catch (err) {
        console.error('Leap parse error:', err);
    }
};
leapWs.onclose = () => {
    state.leapConnected = false;
    document.getElementById('leap-dot').classList.remove('on');
    console.log('Ultraleap disconnected');
};

// ============================================================================
// RENDER LOOP
// ============================================================================

function animate() {
    requestAnimationFrame(animate);
    
    // Update camera rig from XR50
    cameraRig.position.copy(state.headPos);
    cameraRig.rotation.copy(state.headRot);
    
    // Animate cubes
    scene.traverse(obj => {
        if (obj.userData.rotSpeed) {
            obj.rotation.y += obj.userData.rotSpeed;
            obj.rotation.x += obj.userData.rotSpeed * 0.5;
        }
    });
    
    // Get actual canvas dimensions
    const w = renderer.domElement.width;
    const h = renderer.domElement.height;
    const halfW = Math.floor(w / 2);
    
    // Render left eye (left half of screen)
    renderer.setViewport(0, 0, halfW, h);
    renderer.setScissor(0, 0, halfW, h);
    renderer.setScissorTest(true);
    renderer.render(scene, cameraLeft);
    
    // Render right eye (right half of screen)
    renderer.setViewport(halfW, 0, halfW, h);
    renderer.setScissor(halfW, 0, halfW, h);
    renderer.render(scene, cameraRight);
    
    // FPS counter
    const now = Date.now();
    if (now - state.lastFpsTime > 1000) {
        state.fps = state.frameCount;
        state.frameCount = 0;
        state.lastFpsTime = now;
        document.getElementById('fps').textContent = `FPS: ${state.fps}`;
    }
}
animate();

// ============================================================================
// CONTROLS
// ============================================================================

let mirrored = true; // Default: mirrored for NorthStar optics

document.addEventListener('keydown', (e) => {
    if (e.key === 'h' || e.key === 'H') {
        document.getElementById('info').classList.toggle('hidden');
    }
    if (e.key === 'f' || e.key === 'F') {
        if (document.fullscreenElement) {
            document.exitFullscreen();
        } else {
            document.body.requestFullscreen();
        }
    }
    if (e.key === 'm' || e.key === 'M') {
        mirrored = !mirrored;
        renderer.domElement.style.transform = mirrored ? 'scaleX(-1)' : 'none';
        console.log('Mirror:', mirrored ? 'ON' : 'OFF');
    }
});

// Handle resize
function onResize() {
    const w = window.innerWidth;
    const h = window.innerHeight;
    renderer.setSize(w, h);
    const eyeAspect = (w / 2) / h;
    cameraLeft.aspect = eyeAspect;
    cameraRight.aspect = eyeAspect;
    cameraLeft.updateProjectionMatrix();
    cameraRight.updateProjectionMatrix();
}
window.addEventListener('resize', onResize);
onResize(); // Call immediately to set correct initial size

console.log('NorthStar VR Ready - Press F for fullscreen, H to hide UI');

