"""Fetch a pinned candidate reference and compile the shared Rust model for the explorer."""
import hashlib
import io
from pathlib import Path
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent
PUBLIC = ROOT.parent / "visual-test/public/hardware"
COMMIT = "76d0f6c09475ad3bf81b5edd614bcd2203d1328f"
URL = f"https://raw.githubusercontent.com/rtenlab/BOXR/{COMMIT}/docs/NSNF_v01.zip"
SHA256 = "20d4f6eda97c5eb617ac371bd8122a614b6cfc47710299b33812279dbb20228f"

def main():
    PUBLIC.mkdir(parents=True, exist_ok=True)
    local_archive = ROOT / 'NSNF_v01.zip'
    archive = local_archive.read_bytes() if local_archive.exists() else urllib.request.urlopen(URL, timeout=30).read()
    if hashlib.sha256(archive).hexdigest() != SHA256:
        raise ValueError("Reference archive checksum mismatch")
    with zipfile.ZipFile(io.BytesIO(archive)) as source:
        for member in source.namelist():
            if member.endswith((".stl", ".step")):
                (PUBLIC / Path(member).name).write_bytes(source.read(member))
    sensor_mount = ROOT / 'NorthStar-Next-XR50-IR170-Mount.stl'
    if sensor_mount.exists():
        (PUBLIC / sensor_mount.name).write_bytes(sensor_mount.read_bytes())
    subprocess.run(["cargo", "build", "--manifest-path", str(ROOT / "model/Cargo.toml"),
                    "--release", "--target", "wasm32-unknown-unknown"], check=True)
    wasm = ROOT / "model/target/wasm32-unknown-unknown/release/vibestar_hardware_model.wasm"
    (PUBLIC / "model.wasm").write_bytes(wasm.read_bytes())
    print(f"Prepared candidate Next geometry and Rust WASM in {PUBLIC}")

if __name__ == "__main__":
    main()
