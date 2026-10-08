# Session terminal

The terminal is enabled by default on macOS and Linux Wayland. Open it with
the terminal icon beside the composer controls in a local session. The panel runs
your `$SHELL` (or `/bin/sh`) in the session's project directory. Hiding the panel,
switching sessions, or closing a pane keeps the shell running. **Stop** closes
the shell; **Restart** replaces it with a fresh shell. Exiting the shell, including
**Ctrl+D** at an empty prompt, closes the drawer and returns focus to the composer.
Opening it again starts a fresh shell at the previous height. The terminal opens
below the composer and follows the app's theme, including live dark/light changes.
**Ctrl + backtick** toggles the terminal in the focused session, including from
the shell.
Opening focuses the shell; hiding returns focus to the composer. The shortcut
can be changed in Settings > Key bindings.
Drag the divider above the terminal to resize it. Each session remembers its
height while hidden or restarting its shell; smaller panes temporarily limit
the height to keep room for the composer.
Terminals end when the app exits and are not restored on restart.

Run `devenv shell cargo run` to build and launch with all dependencies supplied.
For builds without devenv, install the dependencies below.

The terminal uses [gpui-libghostty](https://github.com/behzade/gpui-libghostty).
Building requires Zig 0.16 on `PATH` (or set `ZIG` to its executable). macOS also
requires Xcode command-line tools. Linux requires Wayland, EGL, OpenGL 4.3,
libxml2, and libc++ 21 or newer, including development libraries for linking.
The first build downloads Ghostty's Zig dependencies. On Ubuntu 24.04, install
`libc++-21-dev` and `libc++abi-21-dev` from [LLVM's APT repository](https://apt.llvm.org/),
plus `libxml2-dev` and `libgl-dev`. Linux packages require the corresponding
libc++ 21 and libxml2 runtime libraries.

For a build without the terminal and its dependencies, use
`cargo run --no-default-features`.

SSH, Windows, X11, Web Connect, agent-controlled terminals, and sending terminal
selections into chat are not yet supported. Agent commands still
run through ACP independently of the shell.
