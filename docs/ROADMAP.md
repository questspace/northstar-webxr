# Vibestar roadmap

Vibestar is an open, modular XR stack for Project NorthStar-class optical see-through
headsets: tracking bridges, a WebXR runtime, a native pipeline and a device hub, all in
this monorepo. The direction below is the reason the stack exists.

## 1. Developer loop on the headset (done)

- `northstar-hub`: one local GUI that keeps the headset display laid out, runs the
  tracking bridges, launches the native app or the WebXR page onto the panel and
  hot-reloads the native app while its source changes. WebXR hot-reloads through Vite.
- An agent (or a person) edits `visual-test/src` or `northstar-native/src` and sees the
  change in the headset within seconds, with the laptop screen untouched.

## 2. Hands as the interface (next)

- Hand occlusion is in: a depth-only hand mesh cuts holograms behind the real hand
  (`Hands` occluder in the WebXR app). Finger interaction already works through WebXR
  `select` events from the pinch (see `/examples/cones.html`).
- Next: a hand-interactable 3D menu and grab/scale/rotate of objects, in both the
  WebXR runtime (as a reusable interaction layer on top of `XRHand`) and native
  (LeapC joints are already decoded).
- Persistent object placement: anchors in the SLAM frame (IWER's persistent anchors)
  and, for cross-restart persistence, XR50 CSLAM map save/load (`docs/WEBXR_PLAN.md`).

## 3. Seeing Spaces workbench

A simulated Physics-AI workbench after Bret Victor's *Seeing Spaces*
(https://worrydream.com/SeeingSpaces/): the debugging tools he describes — seeing
inside (every sensor and internal variable of a thing laid out around it), seeing
across time (scrub, compare and replay histories), seeing across possibilities
(parameter sweeps side by side) — as spatial, hand-manipulable instruments around the
physical object, viewed through NorthStar. Built on the same runtime: each tool is a
module that subscribes to a data stream and renders in the shared XR scene.

## 4. QuestSpace pilot

An AI+XR environment for the QuestSpace makerspace in Košice: on-demand digital twins
of machines and projects, shared multi-user space (several NorthStars or AI glasses see
the same anchored scene), Dynamicland-style spatial programming with AI agents editing
the running scene. The pieces that have to exist first are the ones above: a reliable
hands-first interface, persistent anchors across devices and sessions, and a device hub
that makes a headset as easy to attach as a monitor.
