"""Check the six generated support meshes and archive completeness."""
import argparse
import json
from pathlib import Path
import tempfile
import zipfile

from check_stl import inspect


PARTS = [f'{side}-{part}' for side in ('left', 'right')
         for part in ('rail-saddle', 'rail-cap', 'rear-temple')]
REFERENCE = '003f03d542307b59f0cda3add5112c88f6bb9b16531c2f554d9d41d97df53150'


def check(path):
    expected = {f'{part}.{extension}' for part in PARTS for extension in ('ofb', 'stp', 'stl')}
    expected.update(('support-recipes.json', 'READ-ME-FIRST.txt'))
    with zipfile.ZipFile(path) as archive, tempfile.TemporaryDirectory() as temporary:
        if set(archive.namelist()) != expected or len(archive.namelist()) != len(expected):
            raise ValueError('Expected six OFB/STEP/STL sets, recipes and instructions')
        if archive.testzip() is not None:
            raise ValueError('Archive CRC check failed')
        recipe = json.loads(archive.read('support-recipes.json'))
        if recipe['source_sha256'] != REFERENCE or recipe['units'] != 'mm':
            raise ValueError('Reference or unit mismatch')
        if [part['name'] for part in recipe['parts']] != PARTS:
            raise ValueError('Recipe part list mismatch')
        reports = {}
        for part in PARTS:
            target = Path(temporary) / f'{part}.stl'
            target.write_bytes(archive.read(target.name))
            report = inspect(target)
            report['file'] = target.name
            if not report['closed_single_oriented_mesh']:
                raise ValueError(f'{part}: invalid mesh topology')
            reports[part] = report
        return {'status': 'Fit prototype; not a fabrication release',
                'source_sha256': REFERENCE, 'meshes': reports}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    report = json.dumps(check(args.archive), indent=2) + '\n'
    if args.output:
        args.output.write_text(report)
    else:
        print(report, end='')
