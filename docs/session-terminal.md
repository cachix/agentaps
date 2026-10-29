# Session terminal

The terminal is enabled by default on macOS and Linux. Click a project's
**+** button and select **Terminal** to start a standalone shell in that project.
Terminal sessions fill their pane and appear with a terminal icon in the sidebar;
coding sessions show their agent's icon before the status indicator. Terminal
session entries survive app restarts, but their shells do not. Select **Open
terminal** to start a fresh shell.

You can also open a terminal drawer with
the terminal icon at the bottom-right corner beneath the composer in a local session. The panel runs
your `$SHELL` (or `/bin/sh`) in the session's project directory. Hiding the panel,
switching sessions, or closing a pane keeps the shell running. **Stop** closes
the shell; **Restart** replaces it with a fresh shell. Exiting the shell, including
**Ctrl+D** at an empty prompt, closes the drawer and returns focus to the composer.
Opening it again starts a fresh shell at the previous height. The terminal opens
below the composer and follows the app's theme, using its monospace font at
the smaller code-text size (13 px at default zoom). Generic Monospace aliases
and missing theme fonts resolve to an installed monospace family to keep
columns aligned. The terminal follows live dark/light changes and app zoom.
**Ctrl+T** toggles the terminal in the focused session, including from
the shell.
Opening focuses the shell; hiding returns focus to the composer. The shortcut
can be changed in Settings > Key bindings.
Drag the divider above the terminal to resize it. Each session remembers its
height while hidden or restarting its shell; smaller panes temporarily limit
the height to keep room for the composer.
Shell processes end when the app exits and are not restored on restart.

## Build requirements

Run `devenv shell cargo run` to build and launch with all dependencies supplied.
For builds without devenv, install the dependencies below.

The terminal uses [Rio's embeddable Rust core](https://docs.rs/rio-vt/0.5.28/rio_vt/)
for VT parsing, scrollback, and PTY management, and GPUI for rendering.
The reusable [gpui-rio workspace crate](../crates/gpui-rio/README.md) owns the
terminal adapter. Agentaps supplies its colors, monospace font, and zoom while
keeping session and drawer controls in the app.
It needs no Zig, libc++, libxml2, or separate terminal application. macOS requires
Xcode command-line tools. Linux requires the GPUI and theme development libraries
listed in the [source build guide](../README.md#build-from-source).

For a build without the terminal and its dependencies, use
`cargo run --no-default-features`, `cargo install agentaps --locked --no-default-features`,
or `cargo install --path . --locked --no-default-features` for a checkout.
GPUI and theme development libraries are still required, as listed in the
[source build guide](../README.md#build-from-source).

## Linux package prerequisites

New builds need only the app's GTK, Qt, and audio runtime dependencies. Install
the matching DEB with APT so those packages are resolved automatically:

```sh
sudo apt update
sudo apt install ./agentaps_VERSION_amd64.deb
```

Replace `VERSION` with the release version. Ubuntu 26.04 packages retain the
`_ubuntu26.04_amd64.deb` suffix and are tested on Ubuntu 26.04. Neither new build
requires LLVM's APT repository, libc++, or libxml2 for the terminal.
Older Ghostty-based releases through 0.6.2 still need their original terminal
runtime dependencies; these new requirements apply to Rio-based builds.

## Input and scrollback

Use the mouse wheel to scroll the 10,000-line history. Drag to select text.
Copy and paste use **Ctrl+Shift+C/V** on Linux and **Cmd+C/V** on macOS.
**Ctrl+C** sends an interrupt to the shell. Applications requesting bracketed
paste receive pasted text between the standard paste delimiters.

## Current limitations

SSH, Windows, Web Connect, agent-controlled terminals, and sending terminal
selections into chat are not yet supported. The initial GPUI renderer does not
display terminal graphics or forward application mouse reporting. Agent commands
still run through ACP independently of the shell.
