# Desktop build candidates

The `Desktop build candidates` workflow compiles the desktop app on Linux x64,
Windows x64, macOS Intel, and macOS Apple Silicon. Every pull request that
changes the desktop build or packaging runs it. You can also start it manually
from GitHub Actions. Each successful job uploads an archive and its SHA-256
checksum as workflow artifacts. The website only links to assets on the latest
published GitHub release, so candidate artifacts do not appear there.

The current candidates are a Linux tarball, a portable Windows ZIP, and a macOS
DMG containing an app bundle. The macOS app has an ad hoc signature for local
testing. It is not Developer ID signed or notarized. The Windows executable is
unsigned. Test installation, launch, agent discovery, pairing, and SSH sessions
on each platform before publishing these builds. In particular, check that a
Mac app launched from Finder can find the user's installed agent commands.

## Prepare a release

1. Update the version in `Cargo.toml` and `Cargo.lock`. Move the relevant
   `CHANGELOG.md` entries from `Unreleased` to a dated version heading, then
   leave `Unreleased` ready for future changes.
2. Merge that release preparation and tag the resulting commit as `vX.Y.Z`.
   The packaging script rejects a tag that does not match the Cargo version.
3. Run `Desktop build candidates` on that tag. Download all four artifacts
   from the successful workflow run and check each `.sha256` file against its
   archive.
4. Sign and notarize the macOS build and sign the Windows build, or explicitly
   decide to distribute unsigned builds. Smoke test the exact files you intend
   to publish on supported systems.
5. Create the GitHub release and attach the tested archives. Confirm the site
   presents the expected platform links. No release is created by the candidate
   workflow.

For a local Linux packaging check, build the desktop binary and package it:

```sh
devenv shell cargo build --release --locked
devenv shell python3 scripts/release/package.py --target x86_64-unknown-linux-gnu \
  --binary target/release/agentaps
```

The local archive and checksum are written to `dist/desktop/`.
