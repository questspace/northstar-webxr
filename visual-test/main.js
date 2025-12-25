import * as THREE from 'three';

// Config
const WS_URL = `ws://${location.host}`;
const GROUND_SIZE = 40;
const EYE_HEIGHT = 1.6;

// State
let pose = { x: 0, y: 0, z: 0, roll: 0, pitch: 0, yaw: 0 };

// Three.js setup
const renderer = new THREE.WebGLRenderer({ canvas: document.getElementById('canvas'), antialias: true });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.shadowMap.enabled = true;

const scene = new THREE.Scene();
scene.background = new THREE.Color(0x1a1a2e);
scene.fog = new THREE.Fog(0x1a1a2e, 10, 50);

const camera = new THREE.PerspectiveCamera(60, innerWidth / innerHeight, 0.1, 100);

// Lighting
scene.add(new THREE.AmbientLight(0x404060, 0.5));
const sun = new THREE.DirectionalLight(0xfff0dd, 1.5);
sun.position.set(10, 20, 10);
sun.castShadow = true;
scene.add(sun);

// Ground
const ground = new THREE.Mesh(
    new THREE.PlaneGeometry(GROUND_SIZE, GROUND_SIZE),
    new THREE.MeshStandardMaterial({ color: 0x2d5a27, roughness: 0.9 })
);
ground.rotation.x = -Math.PI / 2;
ground.receiveShadow = true;
scene.add(ground);

scene.add(new THREE.GridHelper(GROUND_SIZE, GROUND_SIZE, 0x3d7a37, 0x2d5a27));

// Headset avatar
const headset = createHeadset();
scene.add(headset);

function createHeadset() {
    const group = new THREE.Group();
    const mat = (color, emissive = 0) => new THREE.MeshStandardMaterial({ 
        color, roughness: 0.3, metalness: 0.8, emissive, emissiveIntensity: 0.5 
    });
    
    // Visor
    group.add(Object.assign(
        new THREE.Mesh(new THREE.BoxGeometry(0.18, 0.1, 0.12), mat(0x1a1a1a)),
        { castShadow: true }
    ));
    
    // Glass
    const glass = new THREE.Mesh(new THREE.BoxGeometry(0.16, 0.06, 0.01), mat(0x00ff88, 0x00ff88));
    glass.position.z = 0.065;
    group.add(glass);
    
    // Direction arrow
    const arrow = new THREE.Mesh(new THREE.ConeGeometry(0.02, 0.06, 8), mat(0x00ff88, 0x00ff88));
    arrow.rotation.x = Math.PI / 2;
    arrow.position.z = 0.12;
    group.add(arrow);
    
    return group;
}

// WebSocket connection
function connect() {
    const ws = new WebSocket(WS_URL);
    
    ws.onopen = () => updateStatus('connected');
    ws.onclose = () => {
        updateStatus('disconnected');
        setTimeout(connect, 1000);
    };
    ws.onmessage = (e) => {
        try {
            pose = JSON.parse(e.data);
            updateUI();
        } catch {}
    };
}

// UI updates
const $ = (id) => document.getElementById(id);
const fmt = (n) => n.toFixed(2);

const updateStatus = (status) => $('status').className = status;
const updateUI = () => {
    $('position').textContent = `X: ${fmt(pose.x)} Y: ${fmt(pose.y)} Z: ${fmt(pose.z)}`;
    $('rotation').textContent = `R: ${fmt(pose.roll)}° P: ${fmt(pose.pitch)}° Y: ${fmt(pose.yaw)}°`;
};

// Render loop
const deg2rad = THREE.MathUtils.degToRad;

function animate() {
    requestAnimationFrame(animate);
    
    // Apply pose
    headset.position.set(pose.x, pose.y + EYE_HEIGHT, pose.z);
    headset.rotation.set(deg2rad(pose.pitch), deg2rad(pose.yaw), deg2rad(pose.roll), 'YXZ');
    
    // Follow camera
    const offset = new THREE.Vector3(0, 2, 5).applyEuler(new THREE.Euler(0, deg2rad(pose.yaw), 0));
    camera.position.lerp(headset.position.clone().add(offset), 0.05);
    camera.lookAt(headset.position);
    
    renderer.render(scene, camera);
}

// Resize handler
window.onresize = () => {
    camera.aspect = innerWidth / innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(innerWidth, innerHeight);
};

// Start
connect();
animate();
