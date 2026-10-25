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

## 2. Hands as the interface (built, to be checked on the head)

- WebXR interaction layer (`visual-test/src/xr/interaction.ts`): pinch to grab (one hand
  moves and turns, two hands scale), fingertip presses, slider grabs, hover; works on any
  WebXR device with hands. UI kit on top (`ui.ts`): buttons, a palm-summoned menu,
  sliders. Hand occluders from the joints (`hands.ts`). Demo: `/examples/menu.html`.
- Native: the same grab / two-hand scale / press rules and depth-only occluders in
  `northstar-native/src/interact.rs` (cubes to grab, warp / re-projection / reset buttons).
- Persistent placement through WebXR anchors with persistent handles (`anchors.ts`):
  objects return after a reload while the bridge keeps the SLAM frame alive. Across a
  headset power cycle the frame restarts; XR50 CSLAM map save/load is still the fix
  (`docs/WEBXR_PLAN.md`).
- A mouse-driven synthetic hand (`?hand=mouse`) exercises all of it without the headset;
  the interactions above were verified that way. What is left is wearing it.

## 3. Seeing Spaces workbench (built: `docs/WORKBENCH.md`)

A simulated Physics-AI workbench after Bret Victor's *Seeing Spaces*
(https://worrydream.com/SeeingSpaces/): a self-balancing robot on a bench with the
debugging tools he describes as hand-operated instruments around it — seeing inside
(every sensor and controller variable, placed where it lives), seeing across time
(scrub the last ten seconds, pin and compare moments), seeing across possibilities
(a Kp × Kd sweep of the last push you can touch). `/examples/workbench.html`; each
instrument is a module reading the same frame record and history. Next: a learned policy
in place of the PID, and real hardware streaming its telemetry into the same record.

## 4. QuestSpace pilot

An AI+XR environment for the QuestSpace makerspace in Košice: on-demand digital twins
of machines and projects, shared multi-user space (several NorthStars or AI glasses see
the same anchored scene), Dynamicland-style spatial programming with AI agents editing
the running scene. The pieces that have to exist first are the ones above: a reliable
hands-first interface, persistent anchors across devices and sessions, and a device hub
that makes a headset as easy to attach as a monitor.
