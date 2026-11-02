---
title: Build from source
description: Build and install Agentaps from crates.io or a checkout.
---

Install Rust and the native build dependencies before running Cargo. The built-in terminal uses Rio's Rust core and GPUI rendering, with no separate Zig or Ghostty toolchain. See the [terminal guide](/terminal/#build-requirements).

On Linux, GPUI and theme probing require native development libraries, including fontconfig, FreeType, XKB, Wayland, GTK 4.10 or later, and Qt 6 Widgets. On Ubuntu 24.04, install:

```sh
sudo apt-get install build-essential pkg-config \
  libfontconfig-dev libfreetype6-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libxcb-randr0-dev libxcb-xinput-dev \
  libegl1-mesa-dev libgles2-mesa-dev libglib2.0-dev libgtk-4-dev qt6-base-dev libasound2-dev
```

`libfontconfig-dev` supplies the native fontconfig dependency; a distro-packaged Rust fontconfig crate is not required.

With the dependencies installed, install the current release from crates.io:

```sh
cargo install agentaps --locked
```

To install without the terminal, use:

```sh
cargo install agentaps --locked --no-default-features
```

The GPUI and theme libraries listed above are still required. To build and install from a checkout:

```sh
git clone https://github.com/domenkozar/agentaps.git
cd agentaps
cargo install --path . --locked
```

The checkout follows `main`; to build a published release, check out its `vX.Y.Z` tag before installing. To omit the terminal from a checkout build, use `cargo install --path . --locked --no-default-features`.

To run this checkout with [devenv](https://devenv.sh/) installed:

```sh
devenv shell cargo run --release
```

The development environment provides Rust, GPUI's native libraries, and Node for optional ACP adapters. To build the static website and Web Connect UI in `web/`:

```sh
devenv shell -- bash web/build.sh
```

The output is in `web/dist/`. See the [web deployment guide](https://github.com/cachix/agentaps/blob/main/docs/web-deployment.md) for hosting and local testing, and the [desktop build guide](https://github.com/cachix/agentaps/blob/main/docs/releasing.md) for package builds and releases.

For local Claude ACP sessions, Agentaps uses the Claude Code CLI at `CLAUDE_CODE_EXECUTABLE`, at the path set in Settings > Agents, or found on `PATH`. When a CLI path is available, Agentaps omits the inherited `ANTHROPIC_API_KEY` so Claude Code can use its configured authentication.

