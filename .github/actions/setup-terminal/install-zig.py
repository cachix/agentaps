"""Install a checksum-verified Zig release for native desktop CI builds."""

import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import urllib.request

version = "0.16.0"
architecture = {"arm64": "aarch64", "AMD64": "x86_64"}.get(
    platform.machine(), platform.machine()
)
system = {"Darwin": "macos", "Linux": "linux"}[platform.system()]
with urllib.request.urlopen("https://ziglang.org/download/index.json") as response:
    release = json.load(response)[version][f"{architecture}-{system}"]

install_dir = Path(os.environ["RUNNER_TEMP"]) / f"zig-{version}"
install_dir.mkdir(parents=True, exist_ok=True)
archive = install_dir / "zig.tar.xz"
with urllib.request.urlopen(release["tarball"]) as response:
    archive.write_bytes(response.read())
if hashlib.sha256(archive.read_bytes()).hexdigest() != release["shasum"]:
    raise SystemExit("Zig archive checksum mismatch")
subprocess.run(
    ["tar", "-xf", str(archive), "-C", str(install_dir), "--strip-components=1"],
    check=True,
)
with open(os.environ["GITHUB_PATH"], "a") as output:
    output.write(f"{install_dir}\n")
with open(os.environ["GITHUB_ENV"], "a") as output:
    output.write(f"ZIG={install_dir / 'zig'}\n")
subprocess.run([str(install_dir / "zig"), "version"], check=True)
