import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

// ============================================================================
// SCENE SETUP
// ============================================================================

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 2000);
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setClearColor(0x0a0a0a);
document.body.appendChild(renderer.domElement);

// Controls
const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;
camera.position.set(0, 400, 600);
camera.lookAt(0, 150, 0);

// Lighting
scene.add(new THREE.AmbientLight(0xffffff, 0.5));
const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
dirLight.position.set(1, 2, 1);
scene.add(dirLight);

// Ground grid
const grid = new THREE.GridHelper(1000, 50, 0x333333, 0x222222);
scene.add(grid);

// UI Elements
const xr50Status = document.getElementById('xr50-status');
const leapStatus = document.getElementById('leap-status');
const headPosEl = document.getElementById('headPos');
const headRotEl = document.getElementById('headRot');
const leftHandEl = document.getElementById('leftHand');
const rightHandEl = document.getElementById('rightHand');
const fpsEl = document.getElementById('fps');

// ============================================================================
// HEADSET (XR50)
// ============================================================================

// Create headset mesh (simple NorthStar-style visor)
const headsetGroup = new THREE.Group();

// Visor frame
const visorGeom = new THREE.BoxGeometry(180, 60, 80);
const visorMat = new THREE.MeshPhongMaterial({ 
    color: 0x1a1a2e, 
    emissive: 0x0066ff, 
    emissiveIntensity: 0.2 
});
const visor = new THREE.Mesh(visorGeom, visorMat);
headsetGroup.add(visor);

// Left lens
const lensGeom = new THREE.CircleGeometry(25, 32);
const lensMat = new THREE.MeshPhongMaterial({ 
    color: 0x00aaff, 
    emissive: 0x00aaff, 
    emissiveIntensity: 0.5,
    side: THREE.DoubleSide 
});
const leftLens = new THREE.Mesh(lensGeom, lensMat);
leftLens.position.set(-40, 0, 41);
headsetGroup.add(leftLens);

// Right lens
const rightLens = new THREE.Mesh(lensGeom, lensMat.clone());
rightLens.position.set(40, 0, 41);
headsetGroup.add(rightLens);

// Direction indicator (forward arrow)
const arrowGeom = new THREE.ConeGeometry(10, 30, 8);
const arrowMat = new THREE.MeshPhongMaterial({ color: 0x00ff00 });
const arrow = new THREE.Mesh(arrowGeom, arrowMat);
arrow.rotation.x = Math.PI / 2;
arrow.position.z = 60;
headsetGroup.add(arrow);

headsetGroup.position.y = 200; // Default height
scene.add(headsetGroup);

// ============================================================================
// HANDS (ULTRALEAP)
// ============================================================================

const jointMat = new THREE.MeshPhongMaterial({ color: 0x00ff88, emissive: 0x00ff88, emissiveIntensity: 0.4 });
const palmMat = new THREE.MeshPhongMaterial({ color: 0xff6600, emissive: 0xff6600, emissiveIntensity: 0.4 });
const jointGeom = new THREE.SphereGeometry(8, 12, 12);
const palmGeom = new THREE.SphereGeometry(20, 16, 16);

const hands = { left: null, right: null };

function createHand() {
    const hand = new THREE.Group();
    const palm = new THREE.Mesh(palmGeom, palmMat.clone());
    palm.name = 'palm';
    hand.add(palm);
    
    const joints = [];
    for (let i = 0; i < 20; i++) {
        const joint = new THREE.Mesh(jointGeom, jointMat.clone());
        joints.push(joint);
        hand.add(joint);
    }
    hand.userData.joints = joints;
    hand.visible = false;
    return hand;
}

hands.left = createHand();
hands.right = createHand();
scene.add(hands.left);
scene.add(hands.right);

// ============================================================================
// COORDINATE TRANSFORMS
// ============================================================================

// XR50 position: scale up for better visibility (XR50 outputs in meters, we use mm)
const XR50_SCALE = 500;

function xr50ToThree(x, y, z) {
    return new THREE.Vector3(x * XR50_SCALE, y * XR50_SCALE + 200, -z * XR50_SCALE);
}

// Leap coordinates to Three.js (mirror X, flip Z)
function leapToThree(pos) {
    return new THREE.Vector3(-(pos[0] ?? 0), pos[1] ?? 0, -(pos[2] ?? 0));
}

// ============================================================================
// WEBSOCKET CONNECTIONS
// ============================================================================

let xr50FrameCount = 0;
let leapFrameCount = 0;
let lastFpsUpdate = Date.now();

// XR50 WebSocket (port 8081 from server.js)
const xr50Ws = new WebSocket('ws://localhost:8081');

xr50Ws.onopen = () => {
    console.log('XR50 connected');
    xr50Status.classList.replace('disconnected', 'connected');
};

xr50Ws.onmessage = (e) => {
    try {
        const data = JSON.parse(e.data);
        xr50FrameCount++;
        
        // Update headset position
        headsetGroup.position.copy(xr50ToThree(data.x, data.y, data.z));
        
        // Update headset rotation (degrees to radians)
        const roll = THREE.MathUtils.degToRad(data.roll);
        const pitch = THREE.MathUtils.degToRad(data.pitch);
        const yaw = THREE.MathUtils.degToRad(data.yaw);
        headsetGroup.rotation.set(pitch, yaw, roll, 'YXZ');
        
        // Update UI
        headPosEl.textContent = `${data.x.toFixed(2)}, ${data.y.toFixed(2)}, ${data.z.toFixed(2)}`;
        headRotEl.textContent = `${data.roll.toFixed(0)}° ${data.pitch.toFixed(0)}° ${data.yaw.toFixed(0)}°`;
    } catch (err) { /* skip */ }
};

xr50Ws.onclose = () => {
    xr50Status.classList.replace('connected', 'disconnected');
    headPosEl.textContent = '—';
    headRotEl.textContent = '—';
};

xr50Ws.onerror = () => xr50Status.classList.add('disconnected');

// Ultraleap WebSocket (port 6437)
const leapWs = new WebSocket('ws://localhost:6437/v6.json');

leapWs.onopen = () => {
    console.log('Ultraleap connected');
    leapStatus.classList.replace('disconnected', 'connected');
    leapWs.send(JSON.stringify({ enableGestures: true }));
};

leapWs.onmessage = (e) => {
    try {
        const frame = JSON.parse(e.data);
        if (!frame.hands) return;
        
        leapFrameCount++;
        
        let leftHand = null, rightHand = null;
        for (const h of frame.hands) {
            if (h.type === 'left') leftHand = h;
            else if (h.type === 'right') rightHand = h;
        }
        
        updateHand(leftHand, hands.left, frame.pointables);
        updateHand(rightHand, hands.right, frame.pointables);
        
        leftHandEl.textContent = leftHand ? '✓' : '—';
        rightHandEl.textContent = rightHand ? '✓' : '—';
    } catch (err) { /* skip */ }
};

leapWs.onclose = () => {
    leapStatus.classList.replace('connected', 'disconnected');
    leftHandEl.textContent = '—';
    rightHandEl.textContent = '—';
};

leapWs.onerror = () => leapStatus.classList.add('disconnected');

// Update hand visualization
function updateHand(handData, handObj, pointables) {
    if (!handData) {
        handObj.visible = false;
        return;
    }
    
    handObj.visible = true;
    
    const palm = handObj.getObjectByName('palm');
    if (palm && handData.palmPosition) {
        palm.position.copy(leapToThree(handData.palmPosition));
    }
    
    const fingers = pointables?.filter(p => p.handId === handData.id) || [];
    fingers.sort((a, b) => (a.type || 0) - (b.type || 0));
    
    let idx = 0;
    for (const finger of fingers) {
        for (const pos of [finger.mcpPosition, finger.pipPosition, finger.dipPosition, finger.tipPosition]) {
            if (pos && idx < 20) {
                handObj.userData.joints[idx].position.copy(leapToThree(pos));
                handObj.userData.joints[idx].visible = true;
                idx++;
            }
        }
    }
    
    for (let i = idx; i < 20; i++) {
        handObj.userData.joints[i].visible = false;
    }
}

// ============================================================================
// ANIMATION LOOP
// ============================================================================

function animate() {
    requestAnimationFrame(animate);
    controls.update();
    
    const now = Date.now();
    if (now - lastFpsUpdate > 1000) {
        fpsEl.textContent = `${xr50FrameCount + leapFrameCount}`;
        xr50FrameCount = 0;
        leapFrameCount = 0;
        lastFpsUpdate = now;
    }
    
    renderer.render(scene, camera);
}
animate();

// Handle resize
window.addEventListener('resize', () => {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
});

