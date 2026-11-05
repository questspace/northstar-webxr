# ClassCAD integration contract

Use [ClassCAD](https://classcad.ch/docs/wasm/) as the editable solid-model authority. A valid account token or licensed engine is required. No credentials are committed. The localhost browser engine successfully imported nine placed part instances and exported OFB on 2026-11-04. The local export is in `local/northstar-next-reference.ofb` (ignored by Git). It is an imported reference solid model, not a redesigned parametric frame.

Import the pinned `NSNF_v01-assm.step` using the documented `api.v1.common.load` STP text path. Verify import units (STEP declares them; do not assume STL units), topology, component identity and assembly transforms. Save the baseline in OFB. STEP import supplies geometry, not the original parametric design history.

Define a named assembly datum and document conversion to the app's right-handed coordinates. Store each optical component transform as a full rigid 4×4 matrix with units, source hash and verification status. Lock those transforms. Optical/display changes must be a separate explicit revision with recalibration; a cover or temple edit must not move them.

New features: optical carrier interfaces; screw-hinge mounts sized to purchased hinge drawings; left/right temples with cable channel and bend keep-outs; removable brow electronics tray; covers; nose support; ventilated brick with separate battery and compute mounting. Do not hard-code mounting holes from photographs.

Persist OFB for editable features and STEP for exchange; export individual verified printable solids to STL with declared units and tessellation tolerance. Check watertightness, minimum walls, screw access, overhangs, clearances and optical-transform invariance before labeling an export printable. GLB visualization export must preserve stable component IDs; confirm the installed engine's actual export API before wiring it into the app.

The [ClassCAD MCP](https://classcad.ai/llms.txt) supports agent workflows after account authentication. The public WASM token is bound to allowed domains; Tauri origin/authentication and offline behavior must be checked. Do not embed a secret engine token in the frontend. Desktop local-file imports should be explicitly selected by the user, not passed arbitrary paths from remote web content.

## Current executable workflow

The React part editor uses `frameRecipe.ts` to generate native ClassCAD features. Its defaults come from `hardware/design-assumptions.json`. Recipe v2 records both part choice and dimensions; the original v1 dimension files remain loadable. Editing a value disables exports until the geometry has been rebuilt and its dimensions checked.

The joint inspector uses separate engine documents for the two solids because each document supports one root part or assembly. Grey is the arm, amber the fork. Rotation is about the shared pivot, and visual separation never changes either CAD document. For the ideal capsule arm, its centreline remains at x >= width/2 throughout 0–90 degrees; subtracting its radius leaves x >= 0, while the fork backing ends at x = -2. Thus backing clearance is at least 2 mm. Tabs retain 0.3 mm axial clearance on each side. This proof excludes cables, fasteners, frame geometry and physical deformation.

The assembly measurement export records the source STEP SHA-256, each returned mesh's bounds and vertex count. Mesh indices are inspection identifiers, not stable semantic component IDs. Placements are baked into mesh coordinates, so these bounds must not be represented as recovered optical transforms.

Validated local files include `vibestar-temple-prototype-mm.stl`, `vibestar-hinge-prototype-mm.stl` and `native-temple-validation.stl`. Run `package_fitprints.py` after every regeneration to package checked default coupons. The native macOS generation/export path has been tested using localhost rather than the unsupported custom Tauri origin. Final wearable geometry and physical qualification remain outstanding.
