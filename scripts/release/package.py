#!/usr/bin/env python3
"""Package a native Agentaps release build as a desktop candidate artifact."""

import argparse
import hashlib
import os
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux-x86_64", "tar.gz"),
    "x86_64-apple-darwin": ("macos-x86_64", "dmg"),
    "aarch64-apple-darwin": ("macos-aarch64", "dmg"),
    "x86_64-pc-windows-msvc": ("windows-x86_64", "zip"),
}


def package_version() -> str:
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["package"]["version"]


def artifact_label(version: str) -> str:
    release_tag = os.environ.get("RELEASE_TAG", "")
    if not release_tag and os.environ.get("GITHUB_REF_TYPE") == "tag":
        release_tag = os.environ.get("GITHUB_REF_NAME", "")
    if release_tag:
        expected = f"v{version}"
        if release_tag != expected:
            raise ValueError(f"release tag {release_tag!r} must match Cargo.toml version {expected!r}")
        return release_tag
    commit = os.environ.get("GITHUB_SHA", "")
    return f"dev-{commit[:8]}" if commit else "dev-local"


def copy_documents(destination: Path) -> None:
    shutil.copy2(ROOT / "README.md", destination / "README.md")
    shutil.copy2(ROOT / "LICENSE", destination / "LICENSE")


def package_linux(binary: Path, archive: Path, name: str) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary) / name
        directory.mkdir()
        shutil.copy2(binary, directory / "agentaps")
        copy_documents(directory)
        with tarfile.open(archive, "w:gz") as output:
            output.add(directory, arcname=name)


def package_macos(binary: Path, archive: Path, version: str) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        staging = Path(temporary)
        bundle = staging / "Agentaps.app"
        executable = bundle / "Contents" / "MacOS"
        resources = bundle / "Contents" / "Resources"
        executable.mkdir(parents=True)
        resources.mkdir()
        shutil.copy2(binary, executable / "agentaps")
        copy_documents(resources)
        with (bundle / "Contents" / "Info.plist").open("wb") as info:
            plistlib.dump(
                {
                    "CFBundleExecutable": "agentaps",
                    "CFBundleIdentifier": "dev.agentaps.app",
                    "CFBundleName": "Agentaps",
                    "CFBundlePackageType": "APPL",
                    "CFBundleShortVersionString": version,
                    "CFBundleVersion": version,
                    "NSHighResolutionCapable": True,
                },
                info,
            )
        subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)
        (staging / "Applications").symlink_to("/Applications")
        subprocess.run(
            ["hdiutil", "create", "-volname", "Agentaps", "-srcfolder", str(staging), "-ov", "-format", "UDZO", str(archive)],
            check=True,
        )


def package_windows(binary: Path, archive: Path, name: str) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary) / name
        directory.mkdir()
        shutil.copy2(binary, directory / "agentaps.exe")
        copy_documents(directory)
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
            for file in sorted(directory.iterdir()):
                output.write(file, arcname=f"{name}/{file.name}")


def write_checksum(archive: Path) -> None:
    digest = hashlib.sha256()
    with archive.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    (archive.parent / f"{archive.name}.sha256").write_text(
        f"{digest.hexdigest()}  {archive.name}\n", encoding="utf-8"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--binary", type=Path, help="override the native Cargo binary path")
    parser.add_argument("--output", type=Path, default=ROOT / "dist" / "desktop")
    parser.add_argument("--check-version", action="store_true")
    args = parser.parse_args()

    version = package_version()
    label = artifact_label(version)
    if args.check_version:
        print(f"Agentaps {version}, artifact label {label}")
        return
    if args.target is None:
        parser.error("--target is required unless --check-version is used")

    platform, extension = TARGETS[args.target]
    binary_name = "agentaps.exe" if platform.startswith("windows") else "agentaps"
    binary = args.binary or ROOT / "target" / args.target / "release" / binary_name
    if not binary.is_file():
        parser.error(f"release binary is missing: {binary}")
    args.output.mkdir(parents=True, exist_ok=True)
    name = f"agentaps-{label}-{platform}"
    archive = args.output / f"{name}.{extension}"
    if platform.startswith("linux"):
        package_linux(binary, archive, name)
    elif platform.startswith("macos"):
        package_macos(binary, archive, version)
    else:
        package_windows(binary, archive, name)
    write_checksum(archive)
    print(archive)


if __name__ == "__main__":
    main()
