"""Validate ClassCAD joint exports and package the two PETG fit coupons."""
import argparse
import json
from pathlib import Path
import zipfile

from check_stl import inspect


ROOT = Path(__file__).resolve().parents[1]


def package(source, destination):
    assumptions = json.loads((ROOT / 'design-assumptions.json').read_text())
    p, hinge = assumptions['temple'], assumptions['hinge']
    expected = {
        'temple': [p['length'], p['width'], p['thickness']],
        'hinge': [p['width'] + 9, p['width'], p['thickness'] + 2 * (
            hinge['axial_clearance_each_side_mm'] + hinge['tab_thickness_mm'])],
    }
    reports, assets = {}, {}
    for part, dimensions in expected.items():
        filename = f'vibestar-{part}-prototype-mm.stl'
        path = source / filename
        report = inspect(path)
        if not report['closed_single_oriented_mesh']:
            raise ValueError(f'{part}: STL is not a single closed oriented mesh')
        if any(abs(a - b) > 0.1 for a, b in zip(report['size_mm'], dimensions)):
            raise ValueError(f'{part}: dimensions differ from the current default recipe; regenerate exports')
        reports[part] = report
        assets[filename] = path.read_bytes()
    # This is a proof for the ideal capsule and fork, not a sampled mesh collision test.
    # For 0 <= angle <= 90 degrees, the capsule centreline has x >= radius.
    # Subtracting its radius leaves x >= 0; the backing ends at x = -2.
    clearance = {
        'model': 'Ideal straight capsule arm rotating about (width/2, width/2)',
        'angle_range_deg': [0, 90],
        'minimum_backing_clearance_mm': 2,
        'axial_clearance_each_side_mm': hinge['axial_clearance_each_side_mm'],
        'excluded': ['screw and washers', 'print distortion', 'cable', 'frame', 'deformation'],
    }
    manifest = {'status': 'FIT COUPONS ONLY; not a complete wearable frame',
                'assumptions': assumptions, 'meshes': reports, 'ideal_joint_clearance': clearance}
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, 'w', zipfile.ZIP_DEFLATED) as archive:
        for filename, data in assets.items():
            archive.writestr(filename, data)
        archive.writestr('manifest.json', json.dumps(manifest, indent=2) + '\n')
        archive.write(ROOT / 'cad/fit-print-instructions.txt', 'READ-ME-FIRST.txt')
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT / 'cad/local')
    parser.add_argument('--output', type=Path, default=ROOT / 'cad/local/vibestar-joint-fitprints.zip')
    args = parser.parse_args()
    result = package(args.source, args.output)
    print(json.dumps({'package': str(args.output.resolve()), 'status': result['status'],
                      'mesh_checks_passed': list(result['meshes'])}, indent=2))
