#!/usr/bin/env python3
"""Publish the release workspace, skipping versions already on crates.io."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
from urllib.error import HTTPError
from urllib.request import Request, urlopen


PUBLISH_ORDER = ("agentaps-control-protocol", "agentaps")


def registry_has_version(name, version):
    # Both workspace crate names use the four-character sparse index layout.
    url = f"https://index.crates.io/{name[:2]}/{name[2:4]}/{name}"
    request = Request(url, headers={"User-Agent": "agentaps-release"})
    try:
        with urlopen(request, timeout=30) as response:
            entries = response.read().decode().splitlines()
    except HTTPError as error:
        if error.code == 404:
            return False
        raise
    for line in entries:
        entry = json.loads(line)
        if entry["vers"] == version:
            if entry["yanked"]:
                raise ValueError(f"{name} {version} is yanked; choose a new version")
            return True
    return False


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--path", type=Path, default=Path.cwd())
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--publish", action="store_true")
    args = parser.parse_args(argv)
    if not re.fullmatch(r"v\d+\.\d+\.\d+", args.tag):
        raise ValueError("the release tag must be a stable vX.Y.Z version")

    checkout = args.path.resolve()
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
            cwd=checkout,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    packages = {p["name"]: p for p in metadata["packages"]}
    if f"v{packages['agentaps']['version']}" != args.tag:
        raise ValueError("the release tag must match the Agentaps Cargo version")

    pending = []
    for name in PUBLISH_ORDER:
        package = packages[name]
        if package["publish"] is not None and "crates-io" not in package["publish"]:
            raise ValueError(f"{name} does not allow publishing to crates.io")
        version = package["version"]
        if registry_has_version(name, version):
            print(f"Skipping {name} {version}: already published", flush=True)
        else:
            pending.append(name)
            print(f"Pending {name} {version}", flush=True)

    if args.dry_run:
        # Validate both archives even when the selected release already exists.
        command = [
            "cargo", "publish", "--workspace", "--dry-run", "--locked",
            "--registry", "crates-io",
        ]
    elif pending:
        command = ["cargo", "publish", "--locked", "--registry", "crates-io"]
        for name in pending:
            command.extend(["--package", name])
    else:
        print("All release crates are already published", flush=True)
        return
    subprocess.run(command, cwd=checkout, check=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Publishing failed: {error}", file=sys.stderr)
        sys.exit(1)
