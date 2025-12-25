import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

// Scene setup
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 2000);
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setClearColor(0x1a1a1a);
document.body.appendChild(renderer.domElement);

// Orbit controls for debugging
const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;

// Lighting
const ambientLight = new THREE.AmbientLight(0xffffff, 0.6);
scene.add(ambientLight);
const directionalLight = new THREE.DirectionalLight(0xffffff, 0.8);
directionalLight.position.set(1, 1, 1);
scene.add(directionalLight);

// Add grid helper for reference (in mm scale)
const gridHelper = new THREE.GridHelper(500, 50, 0x444444, 0x333333);
scene.add(gridHelper);

// Add axis helper
const axisHelper = new THREE.AxesHelper(100);
scene.add(axisHelper);

// Camera position - above and looking down at hands (Leap origin is device center)
camera.position.set(0, 400, 400);
camera.lookAt(0, 150, 0);

// UI elements
const connectionStatus = document.getElementById('connection-status');
const leftHandEl = document.getElementById('leftHand');
const rightHandEl = document.getElementById('rightHand');
const fpsEl = document.getElementById('fps');

// Hand joint materials
const jointMaterial = new THREE.MeshPhongMaterial({ color: 0x00ff88, emissive: 0x00ff88, emissiveIntensity: 0.5 });
const palmMaterial = new THREE.MeshPhongMaterial({ color: 0xff6600, emissive: 0xff6600, emissiveIntensity: 0.5 });

// Joint sizes (in mm to match Leap coordinates)
const jointGeometry = new THREE.SphereGeometry(8, 16, 16);
const palmGeometry = new THREE.SphereGeometry(20, 16, 16);

// Hand objects storage
const hands = { left: null, right: null };

// Create hand with palm + 20 finger joints (5 fingers × 4 joints)
function createHand() {
    const hand = new THREE.Group();
    
    // Palm sphere
    const palm = new THREE.Mesh(palmGeometry, palmMaterial.clone());
    palm.name = 'palm';
    hand.add(palm);
    
    // 20 finger joint spheres
    const joints = [];
    for (let i = 0; i < 20; i++) {
        const joint = new THREE.Mesh(jointGeometry, jointMaterial.clone());
        joints.push(joint);
        hand.add(joint);
    }
    
    hand.userData.joints = joints;
    hand.visible = false;
    return hand;
}

// Create both hands and add to scene
hands.left = createHand();
hands.right = createHand();
scene.add(hands.left);
scene.add(hands.right);

// FPS tracking
let frameCount = 0;
let lastFpsUpdate = Date.now();

// Convert Leap coordinates to Three.js
// Leap: X=right, Y=up (height above device), Z=away from screen
// Three.js: X=right, Y=up, Z=toward viewer
// Mirror X to correct left/right hand orientation
function leapToThree(pos) {
    const x = pos[0] ?? 0;
    const y = pos[1] ?? 0;
    const z = pos[2] ?? 0;
    return new THREE.Vector3(-x, y, -z);  // Mirror X and flip Z
}

// Update hand visualization from Leap frame data
function updateHand(handData, handObj, pointables) {
    if (!handData) {
        handObj.visible = false;
        return;
    }
    
    handObj.visible = true;
    
    // Update palm position
    const palm = handObj.getObjectByName('palm');
    if (palm && handData.palmPosition) {
        palm.position.copy(leapToThree(handData.palmPosition));
    }
    
    // Get finger pointables for this hand
    const handId = handData.id;
    const fingers = pointables ? pointables.filter(p => p.handId === handId) : [];
    
    // Sort by finger type (0=thumb, 1=index, 2=middle, 3=ring, 4=pinky)
    fingers.sort((a, b) => (a.type || 0) - (b.type || 0));
    
    // Update finger joint positions
    let jointIdx = 0;
    for (const finger of fingers) {
        // Each finger has 4 joint positions: MCP → PIP → DIP → TIP
        const positions = [
            finger.mcpPosition,  // Knuckle
            finger.pipPosition,  // First bend
            finger.dipPosition,  // Second bend
            finger.tipPosition   // Fingertip
        ];
        
        for (const pos of positions) {
            if (pos && jointIdx < 20) {
                handObj.userData.joints[jointIdx].position.copy(leapToThree(pos));
                handObj.userData.joints[jointIdx].visible = true;
                jointIdx++;
            }
        }
    }
    
    // Hide any remaining unused joints
    for (let i = jointIdx; i < 20; i++) {
        handObj.userData.joints[i].visible = false;
    }
}

// WebSocket connection to Ultraleap
const ws = new WebSocket('ws://localhost:6437/v6.json');

ws.onopen = () => {
    console.log('Connected to Ultraleap');
    connectionStatus.classList.remove('disconnected');
    connectionStatus.classList.add('connected');
    ws.send(JSON.stringify({ enableGestures: true }));
};

ws.onmessage = (event) => {
    try {
        const frame = JSON.parse(event.data);
        if (!frame.hands) return;
        
        frameCount++;
        
        // Find hands by type
        let leftHand = null, rightHand = null;
        for (const hand of frame.hands) {
            if (hand.type === 'left') leftHand = hand;
            else if (hand.type === 'right') rightHand = hand;
        }
        
        // Update visualizations
        updateHand(leftHand, hands.left, frame.pointables);
        updateHand(rightHand, hands.right, frame.pointables);
        
        // Update UI
        leftHandEl.textContent = leftHand ? '✓' : '—';
        rightHandEl.textContent = rightHand ? '✓' : '—';
        
    } catch (e) {
        // Skip non-JSON messages
    }
};

ws.onclose = () => {
    console.log('Disconnected');
    connectionStatus.classList.remove('connected');
    connectionStatus.classList.add('disconnected');
};

ws.onerror = () => connectionStatus.classList.add('disconnected');

// Animation loop
function animate() {
    requestAnimationFrame(animate);
    controls.update();
    
    // Update FPS display
    const now = Date.now();
    if (now - lastFpsUpdate > 1000) {
        fpsEl.textContent = frameCount;
        frameCount = 0;
        lastFpsUpdate = now;
    }
    
    renderer.render(scene, camera);
}
animate();

// Handle window resize
window.addEventListener('resize', () => {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
});
