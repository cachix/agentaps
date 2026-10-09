# Session terminal

The terminal is enabled by default on macOS and Linux Wayland. Open it with
the terminal icon at the bottom-right corner beneath the composer in a local session. The panel runs
your `$SHELL` (or `/bin/sh`) in the session's project directory. Hiding the panel,
switching sessions, or closing a pane keeps the shell running. **Stop** closes
the shell; **Restart** replaces it with a fresh shell. Exiting the shell, including
**Ctrl+D** at an empty prompt, closes the drawer and returns focus to the composer.
Opening it again starts a fresh shell at the previous height. The terminal opens
below the composer and follows the app's theme, including live dark/light changes.
**Ctrl+T** toggles the terminal in the focused session, including from
the shell.
Opening focuses the shell; hiding returns focus to the composer. The shortcut
can be changed in Settings > Key bindings.
Drag the divider above the terminal to resize it. Each session remembers its
height while hidden or restarting its shell; smaller panes temporarily limit
the height to keep room for the composer.
Terminals end when the app exits and are not restored on restart.

## Build requirements

Run `devenv shell cargo run` to build and launch with all dependencies supplied.
For builds without devenv, install the dependencies below.

The terminal uses [gpui-libghostty](https://github.com/behzade/gpui-libghostty).
Building requires [Zig 0.16](https://ziglang.org/download/) on `PATH` (or set
`ZIG` to the downloaded Zig executable). Check `zig version`, or `"$ZIG" version`
when using `ZIG`, before running Cargo. If Zig is missing, the build fails with
`read Zig version from "zig": No such file or directory`. macOS also
requires Xcode command-line tools. Linux requires Wayland, EGL, OpenGL 4.3,
libxml2, and libc++ 21 or newer, including development libraries for linking.
The first build downloads Ghostty's Zig dependencies. On Ubuntu 24.04, configure
the LLVM repository as shown below, then install the terminal development libraries:

```sh
sudo apt-get install libc++-21-dev libc++abi-21-dev libxml2-dev libgl-dev
```

For a build without the terminal and its dependencies, use
`cargo run --no-default-features`, `cargo install agentaps --locked --no-default-features`,
or `cargo install --path . --locked --no-default-features` for a checkout.
GPUI and theme development libraries are still required, as listed in the
[source build guide](../README.md#build-from-source).

## Linux package prerequisites

Downloaded packages do not require Zig. The Linux DEB requires the libc++ 21,
libc++abi 21, and libxml2 runtime packages in addition to GTK and Qt. On Ubuntu
24.04, configure [LLVM's APT repository](https://apt.llvm.org/) before installing
the DEB so APT can resolve `libc++1-21` and `libc++abi1-21`:

```sh
sudo apt-get update
sudo apt-get install ca-certificates curl
sudo install -d -m 0755 /etc/apt/keyrings
curl --fail --silent --show-error https://apt.llvm.org/llvm-snapshot.gpg.key \
  | sudo tee /etc/apt/keyrings/apt.llvm.org.asc >/dev/null
sudo chmod 0644 /etc/apt/keyrings/apt.llvm.org.asc
echo 'deb [signed-by=/etc/apt/keyrings/apt.llvm.org.asc] https://apt.llvm.org/noble/ llvm-toolchain-noble-21 main' \
  | sudo tee /etc/apt/sources.list.d/llvm-21.list >/dev/null
sudo apt-get update
sudo apt-get install libc++1-21 libc++abi1-21 libxml2
```

Install the downloaded DEB with `sudo apt install ./agentaps_0.5.2_amd64.deb`
from its download directory, replacing the filename for other releases. APT
installs the remaining package dependencies. These LLVM repository commands
are specific to Ubuntu 24.04.

## Current limitations

SSH, Windows, X11, Web Connect, agent-controlled terminals, and sending terminal
selections into chat are not yet supported. Agent commands still
run through ACP independently of the shell.
