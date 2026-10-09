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

To check conversation restoration against an authenticated local harness, run:

```sh
AGENTAPS_SMOKE_COMMAND=codex-acp cargo test --locked harness_history_reloads_after_restart -- --ignored --nocapture
```

Set `AGENTAPS_SMOKE_COMMAND` to another ACP adapter command to check that harness.
The test sends one small prompt in a temporary project, saves session references
without conversation text, restarts the adapter, and verifies that the harness
replays the user message and reply exactly once. It uses the adapter's normal
credentials and session storage. This opt-in test is skipped by the regular suite.

## Prepare a release

GitHub releases provide desktop installers. Publishing a stable GitHub release
starts the `Publish crates` workflow, which validates and uploads the registry
packages from the same tag. Drafts and prereleases do not publish to crates.io.

1. Update the version in `Cargo.toml` and `Cargo.lock`. Move the relevant
   `CHANGELOG.md` entries from `Unreleased` to a dated version heading, then
   leave `Unreleased` ready for future changes.
2. Merge that release preparation and push a `vX.Y.Z` tag. The workflow rejects
   a tag that does not match the Cargo version and creates a draft release with
   packages and checksums after the builds pass.
3. Smoke test the exact files on supported systems. Sign and notarize the macOS
   build and sign the Windows build, or decide to distribute unsigned builds.
   If signed files replace draft assets, replace their checksums too. Publish
   the draft when the files are ready, then update the fallback release label
   and installer links in `web/site/index.html` to match the published assets.
   Check the website download links both with and without the GitHub API available.

## Publish to crates.io

The `Publish crates` workflow uses [crates.io trusted publishing](https://crates.io/docs/trusted-publishing).
Configure a GitHub trusted publisher in the settings of both
[`agentaps`](https://crates.io/crates/agentaps/settings) and
[`agentaps-control-protocol`](https://crates.io/crates/agentaps-control-protocol/settings):

| Field | Value |
| --- | --- |
| Repository owner | `cachix` |
| Repository name | `agentaps` |
| Workflow filename | `publish-crates.yml` |
| Environment | Leave blank |

The workflow exchanges its GitHub OIDC identity for a short-lived publishing
token after package validation. The authentication action revokes the token at
the end of the job. No repository publishing secret is needed.

Publishing a stable GitHub release starts the workflow automatically.
The workflow checks the release tag against the desktop version,
validates both packages, then publishes missing versions in dependency order.
It skips versions already in the registry, so the unchanged protocol crate does
not need a new version for every desktop release and partial uploads can be retried.

To validate the workflow on an existing published release without uploading:

```sh
gh workflow run publish-crates.yml -f tag=vX.Y.Z -F dry_run=true
```

After configuring both trusted publishers, also check the OIDC token exchange
without uploading by adding `-F verify_auth=true` to that command.

To retry an upload, rerun its failed workflow job, or dispatch it with
`-F dry_run=false`. The workflow uses the supplied published tag rather than
the current contents of `main`.

For a local publication or recovery outside Actions:

Use the same release commit and version as the GitHub release. With Cargo 1.97.1
and the native build dependencies installed, validate both packages together:

```sh
devenv shell cargo publish --workspace --dry-run --locked
```

Cargo verifies workspace dependencies using a temporary registry, so this also
works before the protocol crate's first publication. Linux CI runs this check
after the workspace tests. After validation, publish
the workspace with crates.io credentials configured:

```sh
devenv shell cargo publish --workspace --locked
```

Cargo publishes dependencies before dependents. If `agentaps-control-protocol`
0.1.0 is already published and unchanged, publish only `agentaps` with
`devenv shell cargo publish -p agentaps --locked`. Changes to the protocol crate
require a new protocol version and an updated version requirement in the desktop
manifest. Confirm that `cargo install agentaps --version X.Y.Z --locked` installs
the release before updating the public install instructions.

For a local Linux packaging check, install the pinned packager, build the
desktop binary, and create a DEB:

```sh
devenv shell cargo install cargo-packager --version 0.11.8 --locked
devenv shell cargo build --release --locked
devenv shell cargo packager --release --formats deb --out-dir dist/desktop
```

Standard Linux and macOS builds include the session terminal. CI installs Zig
0.16 and the native terminal libraries before building. See the
[terminal guide](session-terminal.md) for source build requirements. Linux DEB installs
require libc++ 21 and libxml2; Ubuntu 24.04 needs the [LLVM APT repository](https://apt.llvm.org/)
for the libc++ runtime packages. Include terminal launch, theme switching, and
Ctrl+D exit in platform smoke tests.

The local package is written to `dist/desktop/`. On a standard Linux host,
replace `deb` with `deb,appimage` to build both Linux formats. AppImage packaging
uses `linuxdeploy`, which does not run directly on NixOS without a compatible
dynamic loader. CI builds it on Ubuntu. The workflow adds checksums to each
package before uploading it.
