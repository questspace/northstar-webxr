# Vibestar hardware redesign

Status: engineering baseline and runnable explorer, not a released mechanical design. The exact optical assembly, electrical rails and wearable ergonomics still need physical verification. Research checked 2026-11-04. User confirmed Northstar Next; photos were inspected and eight frames sampled from the 91.8-second walkaround video.

## Recommended architecture

Keep the panels, curved combiners and their relative transforms in a rigid optical carrier. Attach serviceable covers and temples to that carrier without loading the optics. Keep the existing display driver near the panels until its FPC limits are established. Route a strain-relieved, replaceable cable bundle along one temple, then behind the ear to a belt-mounted compute/battery enclosure. This is independent of a desktop but still tethered to a wearable brick, as requested.

Start with the working x86-64 Windows tracking/display stack. Choose a compact computer only after checking both sensors, the actual 2880×1600 ~89 Hz display timing, the runtime/compositor and target scene performance together. A fast GPU alone does not establish XR compatibility. Pi 5 is an owned experimental target; Jetson is another experimental target. Do not purchase either as a proven SIR170 host yet. Local pose handling and final reprojection remain necessary for a cloud-rendering experiment; measure network jitter, decode time and motion-to-photon latency before relying on it.

The first harness should preserve certified/native video and USB cables inside one sleeve. A single custom electrical cable is a later optimization, after DP/USB lane requirements and power negotiation are known. A passive USB-C splitter cannot provide this architecture. Keep video routing distinct from the sensor USB topology even if they share an outer sleeve.

## What the reference files establish

[BOXR's upstream repository](https://github.com/rtenlab/BOXR) includes `docs/NSNF_v01.zip`, a Next assembly STEP and six printable parts. The pinned archive has SHA-256 `20d4f6eda97c5eb617ac371bd8122a614b6cfc47710299b33812279dbb20228f`. Its preview resembles the optical layout, but differs in electronics mounting. It is a candidate, not proof of exact fit. Individual STLs may have independent print coordinates; the explorer centers one part at a time and does not pretend they form a registered assembly. The STEP must supply assembly transforms after validation. Preserve upstream attribution and establish the archive's applicable license before redistributing derived hardware files.

Original PCB design sources exist for [Next USB hub](https://github.com/CombineReality/ns-next_hs_hub) and [Next RP2040 board](https://github.com/CombineReality/ns-next_rp2040). Use their board outlines, hole coordinates and connector pinouts only after matching revision against the actual boards. The source files allow reuse and inspection without designing a replacement PCB.

## Power investigation and wiring decision

The repository reports resets under shared USB power. Brownout is a hypothesis alongside signal integrity, shared bandwidth, host suspend and driver behavior. First reproduce on the working PC:

1. Record USB descriptors, hub tree and negotiated DP/display mode. Run each sensor, both sensors, then both plus displays at full brightness. Log disconnect timestamps, pose confidence, hand tracking and frame timing.
2. Measure voltage at each consumer and current at each branch, including boot/replug peaks. A slow USB meter can miss a short sag; capture transient minima with suitable instrumentation. Record cable length and loop resistance.
3. Repeat with a known self-powered hub and short known-good sensor cables, keeping the display link separate. Compare ports/controllers. Disable selective suspend as a controlled test, not as proof that power is fixed.
4. Only then specify battery PDOs, converter input ranges, continuous and transient current, connector ratings, and current-limited distribution. Verify no host backfeed, polarity and startup sequencing before connecting the retained boards.

Proposed block wiring (not a pin-level schematic): battery powers compute through its supported input; a separate rated output powers a self-powered USB hub/distribution branch; host USB data enters that hub; XR50 and SIR170 connect as downstream devices. Host DP video reaches the retained display controller through a validated DP Alt Mode path; supply its power only as its documented input permits. Existing Next hub reuse is conditional on schematic/revision inspection. Never inject two supplies onto VBUS or assume FFC contacts are interchangeable.

[Next assembly instructions](https://docs.projectnorthstar.org/project-north-star/northstar-next/assembly-guide) document the DP Alt Mode display path and warn that reversing the backflip display connector can short the display. Preserve and photograph contact orientation before disassembly.

## Form, balance and construction

Size 51 is a lens-size reference, not a total headset width. The retained large panels and curved optics set the minimum envelope; shrinking them to a Ray-Ban silhouette would violate the optical constraint. Use the reference for bridge/temple fit and styling, then wrap the actual envelope.

Do not add ballast by default. For illustration, 200 g located 50 mm ahead of an ear support generates 0.098 N·m. Counterbalancing at 100 mm behind that support would require 100 g. Moving weight to the pocket reduces total head load; it does not itself cancel front torque. Measure nose/ear reactions and slippage with a mass dummy. Include removable rear retention in the prototype until temples alone demonstrate stable fit.

Use separate left/right replaceable temples, metal screw hinges with mechanical stops, and reinforced mounts. Select actual hinge/insert SKUs before specifying holes. Keep cable bending outside the hinge pinch zone, with a service loop and strain relief at both ends. Preserve sensor sight cones, IR emitter clearance, ventilation, connector service space and optical keep-outs. SIR170 must not be bent to follow the frame. Print material, wall thickness and insert tolerances are engineering variables to validate with coupons and load testing, not guessed production values.

## Compute decision gates

| Option | Why investigate | Gate before selection |
| --- | --- | --- |
| x86-64 Windows + integrated GPU | Existing software compatibility baseline | Sustained stereo render timing, output timing, USB stability, battery/thermal envelope |
| Raspberry Pi 5 | Already owned | ARM sensor runtimes, actual compositor performance, suitable active video conversion |
| Jetson Orin Nano | GPU compute experiment | ARM tracking support, DP path, carrier power and cooling, graphics runtime |
| Remote rendering | Offload scene rendering | Local tracking/reprojection, encoder/decoder availability, measured latency and packet-loss behavior |

[Pi 5 power documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html) describes a shared peripheral budget of 1.6 A with suitable 5 V/5 A power, versus 600 mA otherwise. A nominal 65 W bank may not offer 5 V/5 A. Pi's display output is HDMI, so its USB-C power socket cannot directly drive this DP Alt Mode display. Jetson's [developer-kit guide](https://developer.nvidia.com/embedded/learn/get-started-jetson-orin-nano-devkit) likewise distinguishes its DP display output from its data-only USB-C port. Neither board removes the video-adapter problem automatically.

## Explorer

```sh
rustup target add wasm32-unknown-unknown
python3 hardware/prepare.py
cargo test --manifest-path hardware/model/Cargo.toml
cd visual-test
npm ci
npm run hardware:dev
```

Open `http://localhost:5174/hardware.html`. The preparation script fetches pinned reference files and builds a dependency-free Rust WASM model. The viewer uses Three's WebGPU renderer with an explicitly reported WebGL fallback. All power sliders are hypothetical inputs, not measured part specifications. It models constant-power cable loss, converter efficiency and battery energy, not USB negotiation, signal integrity, thermal behavior or electrochemical discharge curves. No frame-rate, runtime or production-readiness claim follows from the model.

The optional “XR50 + SIR170 mount” reference requires the local `hardware/NorthStar-Next-XR50-IR170-Mount.stl` before running preparation. It is not included in the pinned archive or committed files; selecting it on a fresh clone reports a missing asset. The support assembly does not depend on this optional mesh. ClassCAD operations additionally require a public `CLASSCAD_TOKEN` in the repository `.env` with localhost allowed.

The component manifest is `components.json`; unknown dimensions/masses stay unknown. ClassCAD imports nine placed mechanical part instances and exports native OFB, STEP and assembly bounds. The part editor builds a cable-channel temple and mating fork, saves and reloads dimension recipes, and exports OFB/STEP/STL. The separate joint view shows the default parts folding through 90 degrees. These are fit coupons, not a completed glasses frame.

The Tauri v2 macOS app builds and its native ClassCAD generation and STL save dialog have been exercised. Run `npm run hardware:desktop:build`, then `npm run hardware:desktop:launch` from `visual-test`. The launcher reads the public `CLASSCAD_TOKEN` from the repository `.env` without embedding it in the app bundle. The release app serves its UI on a randomly selected localhost port for the ClassCAD public-token origin requirement. ClassCAD needs network access for its engine/licensing; this is not an offline CAD guarantee.

`design-assumptions.json` records the user-authorized starting choices: candidate Next geometry, 64 mm wearer IPD target, PETG, 0.4 mm nozzle, 0.2 mm layers and a 145 mm temple. IPD is a fit target only and does not alter reference optical placements. Tune the fit after printing rather than treating these choices as measured facts.

After exporting default arm and hinge STLs into `hardware/cad/local`, run `python3 hardware/cad/package_fitprints.py`. This verifies one closed oriented mesh per part and expected overall dimensions, then produces `hardware/cad/local/vibestar-joint-fitprints.zip` with both STLs, hashes, assumptions and assembly instructions. It deliberately labels the package as fit coupons. The checks do not certify minimum walls, self-intersection, fatigue or actual fit.

Revision 1 adds six ClassCAD solids: two rail saddles with hinge forks, two clamp caps and two inward-shaped rear temples. The support assembly imports the nine reference instances without changing their placements and verifies the source STEP hash before building. The rear temples have cable channels, ear drops and strap slots, with an assumed 150 mm ear centre spacing. Export all six parts in native OFB, STEP and STL plus their recipes and print instructions in one ZIP. These files use assembly coordinates and must be placed on the slicer bed without scaling. See `cad/support-print-instructions.txt` for the provisional fasteners, shims and assembly order.

Remaining work includes folding stops, comfortable nose support, electronics enclosures, registration of the sensors and optical surfaces, complete wiring netlist, measured profiles and full assembly animations. The candidate STEP does not establish calibrated optical surfaces or the exact modified sensor mount. Do not export the reference STLs as a new verified glasses design.

## Required measurements before mechanical release

Match STEP revision to optic edges, panel planes and mounting points. Record wearer IPD, eye relief, bridge contact, temple spacing/length; all component and cable masses; PCB outlines/heights/hole centers; connector mating and removal volumes; FFC pin count/pitch/contact side/length; optical and sensor-to-eye transforms. Measure the assembled datum structure, not just individual components. Compare optical calibration before/after mounting and after repeated hinge cycles.

Next gates: simultaneous-sensor bench stability; battery-powered full workload; registered CAD interference and sight-cone check; printed coupon and inert fit prototype; optics/calibration verification; sustained thermal/wear test; hinge/cable durability. Production readiness requires those results and a repeatable build record.
