# Desktop build candidates

The `Desktop build candidates` workflow compiles the desktop app on Linux x64,
Windows x64, and macOS Apple Silicon. Every pull request that changes the
desktop build or packaging runs it. You can also start it manually
from GitHub Actions. Each successful job uploads packages and their SHA-256
checksum as workflow artifacts. The website's platform buttons link to each
platform's preview artifact from the successful run on `main`. The artifacts
are ZIP archives that require GitHub sign-in and expire. Published GitHub
release assets replace the preview links when available. Pushing a version tag
runs the same builds, verifies their checksums, and attaches them to a draft
GitHub release. Drafts do not appear on the website.

The current candidates are a Linux DEB and AppImage, a Windows NSIS installer,
and a macOS DMG containing an app bundle. The macOS app has an ad hoc signature
for local testing. It is not Developer ID signed or notarized. The Windows
installer is unsigned. Test installation, launch, agent discovery, pairing, and SSH sessions
on each platform before publishing these builds. In particular, check that a
Mac app launched from Finder can find the user's installed agent commands.

## Prepare a release

1. Update the version in `Cargo.toml` and `Cargo.lock`. Move the relevant
   `CHANGELOG.md` entries from `Unreleased` to a dated version heading, then
   leave `Unreleased` ready for future changes.
2. Merge that release preparation and push a `vX.Y.Z` tag. The workflow rejects
   a tag that does not match the Cargo version and creates a draft release with
   packages and checksums after the builds pass.
3. Smoke test the exact files on supported systems. Sign and notarize the macOS
   build and sign the Windows build, or decide to distribute unsigned builds.
   If signed files replace draft assets, replace their checksums too. Publish
   the draft when the files are ready, then check the website download links.

For a local Linux packaging check, install the pinned packager, build the
desktop binary, and create a DEB:

```sh
devenv shell cargo install cargo-packager --version 0.11.8 --locked
devenv shell cargo build --release --locked
devenv shell cargo packager --release --formats deb --out-dir dist/desktop
```

The local package is written to `dist/desktop/`. On a standard Linux host,
replace `deb` with `deb,appimage` to build both Linux formats. AppImage packaging
uses `linuxdeploy`, which does not run directly on NixOS without a compatible
dynamic loader. CI builds it on Ubuntu. The workflow adds checksums to each
package before uploading it.
