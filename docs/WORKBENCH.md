# Seeing Spaces workbench

A spatial debugging bench after Bret Victor's *Seeing Spaces*: a maker's thing in the
middle, and around it the three kinds of seeing he asks for, as instruments you operate
with your hands through NorthStar.

Page: `http://localhost:5173/examples/workbench.html` (hub: *WebXR page → workbench*).
Code: `visual-test/src/workbench/`.

## The thing

A two-wheeled self-balancing robot (`sim.ts`), simulated at 200 Hz as a cart-pole driven
by wheel torque, with the motor's reaction torque on the body. It carries the sensors a
real one has — gyro, accelerometer tilt, wheel encoders, battery, motor current — with
noise and gyro bias, a complementary-filter tilt estimate, and a PID with a slow position
hold. Every variable of every step goes into one frame record; the instruments only ever
read frame records.

Poke the robot's side with a fingertip and it gets pushed. Weak gains let it fall.

## Seeing inside (`instruments/inside.ts`)

Twelve readouts float beside the robot, sensors on the left, controller on the right. Each
shows the live value, a signed bar, a two-second sparkline, and a leader line to the part
it belongs to: the gyro to the IMU chip, wheel speed to the wheel, the command to the
motor, torque to the axle. Nothing is hidden inside the robot.

## Seeing across time (`instruments/time.ts`, `history.ts`)

A timeline of the last ten seconds — lean, estimate, command, push marks, fallen spans —
with a scrub knob under it. Pinch the knob and the world pauses; the robot and all twelve
readouts move back to that moment. *Pin* keeps a moment as a translucent ghost on the
far track and a marker on the timeline, so the moment before a fall sits next to the
moment after a gain change. *Live* resumes.

## Seeing across possibilities (`instruments/possibilities.ts`, `sweep.ts`)

A Kp × Kd grid: twenty-five replays of the last push, each cell one controller setting,
green where it settles (brighter is faster), red where it falls, the live gains framed.
The sweep re-runs whenever the gains, the noise switch or the push change. Touch a cell
to adopt its gains; slide the Kp / Kd / Ki knobs on the bench to explore between cells.

## The interface

Everything is operated with hands through `visual-test/src/xr/`:

- `interaction.ts`: pinch to grab (one hand moves and turns, two hands scale), fingertip
  presses with hysteresis, rail-constrained grabs for sliders, hover. Runtime-agnostic: it
  reads `renderer.xr.getHand(i).joints`, so it runs on any WebXR device with hands.
- `ui.ts`: labels, push buttons, a menu panel that comes to your left palm when you turn
  it toward you and stays where you park it, sliders.
- `hands.ts`: depth-only hand occluders, so holograms behind the real hand are cut out.
- `anchors.ts`: persistent placement through WebXR anchors (`menu.html` demo).

The bench itself has a grab bar at its front edge; move it to your table.

## Developing without wearing the headset

`?hand=mouse&xr50=0&pitch=-40&warp=0` on any example page: the right hand follows the
mouse (Shift: left hand), the wheel sets its depth, the mouse button pinches, and the head
is parked looking down at the bench. `window.devHand.set('right', { tip, pinch })` and
`window.bench` are scriptable, which is how the interactions are tested by an agent:

```js
bench.push(1); bench.scrub(-2); bench.params.kp = 0.3   // from the console
```

Keys: Space pause, P / Shift+P push, [ ] scrub, I pin, K reset, N noise, U warp, R
re-projection, O occluders.

## Where it goes next

- Swap the PID for a learned policy: `sim.ts` exposes the plant and the sensor model, so a
  policy that reads `Sensors` and writes a command drops in, and the same three instruments
  show what it does inside, over time and across its parameters.
- Real hardware: the frame record is the interface. A robot streaming its telemetry over
  WebSocket replaces `createSim`, and the sweep becomes a model-based prediction next to
  the real trace.
- Several benches in one room (QuestSpace): anchors from `anchors.ts` place each bench; the
  history is per bench, the instruments are shared modules.
