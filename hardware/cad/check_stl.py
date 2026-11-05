"""Check exported STL topology and dimensions; does not certify fit or strength."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import struct


def inspect(path, tolerance=1e-5):
    data = path.read_bytes()
    count = struct.unpack_from('<I', data, 80)[0] if len(data) >= 84 else 0
    if len(data) == 84 + 50 * count:
        faces = [struct.unpack_from('<9f', data, 84 + i * 50 + 12) for i in range(count)]
    else:
        vertices = [tuple(map(float, line.split()[1:])) for line in data.decode().splitlines()
                    if line.strip().startswith('vertex ')]
        if len(vertices) % 3:
            raise ValueError('Incomplete ASCII STL triangles')
        faces = [sum(vertices[i:i + 3], ()) for i in range(0, len(vertices), 3)]
    if not faces or not all(len(f) == 9 and all(math.isfinite(x) for x in f) for f in faces):
        raise ValueError('Empty or invalid STL')
    vertex_ids, points, triangles = {}, [], []
    for face in faces:
        triangle = []
        for i in (0, 3, 6):
            point = face[i:i + 3]
            key = tuple(round(x / tolerance) for x in point)
            if key not in vertex_ids:
                vertex_ids[key] = len(points)
                points.append(point)
            triangle.append(vertex_ids[key])
        triangles.append(triangle)
    edges, directed, neighbours = Counter(), Counter(), defaultdict(set)
    volume, degenerate = 0, 0
    for a, b, c in triangles:
        pa, pb, pc = points[a], points[b], points[c]
        u, v = [pb[i] - pa[i] for i in range(3)], [pc[i] - pa[i] for i in range(3)]
        cross = [u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0]]
        degenerate += sum(x*x for x in cross) < tolerance**4
        volume += (pa[0]*(pb[1]*pc[2]-pb[2]*pc[1]) + pa[1]*(pb[2]*pc[0]-pb[0]*pc[2]) + pa[2]*(pb[0]*pc[1]-pb[1]*pc[0])) / 6
        for x, y in ((a, b), (b, c), (c, a)):
            edges[tuple(sorted((x, y)))] += 1
            directed[(x, y)] += 1
            neighbours[x].add(y)
            neighbours[y].add(x)
    remaining, components = set(range(len(points))), 0
    while remaining:
        stack = [remaining.pop()]
        components += 1
        while stack:
            for neighbour in neighbours[stack.pop()] & remaining:
                remaining.remove(neighbour)
                stack.append(neighbour)
    minimum = [min(p[i] for p in points) for i in range(3)]
    maximum = [max(p[i] for p in points) for i in range(3)]
    boundary = sum(n == 1 for n in edges.values())
    nonmanifold = sum(n != 2 for n in edges.values())
    inconsistent = sum(directed[(a, b)] != 1 or directed[(b, a)] != 1 for a, b in edges)
    return dict(file=str(path.resolve()), sha256=hashlib.sha256(data).hexdigest(),
                units='mm assumed; STL has no unit metadata', triangles=len(triangles), vertices=len(points),
                bounds_mm=[minimum, maximum], size_mm=[maximum[i]-minimum[i] for i in range(3)],
                signed_volume_mm3=volume, connected_components=components,
                boundary_edges=boundary, nonmanifold_edges=nonmanifold,
                inconsistent_edge_winding=inconsistent, degenerate_triangles=degenerate,
                closed_single_oriented_mesh=not (nonmanifold or inconsistent or degenerate) and components == 1 and volume > 0,
                limitations=['Self-intersection, minimum wall, fit, load and print process are not checked.'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stl', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = inspect(args.stl)
    report = json.dumps(result, indent=2)
    if args.output:
        args.output.write_text(report + '\n')
    print(report)
    raise SystemExit(0 if result['closed_single_oriented_mesh'] else 1)
