# gpui-rio

A Rio terminal component for GPUI applications on Linux and macOS. Rio owns VT
parsing, scrollback, and PTY management; GPUI draws the grid and handles input.

Following [gpui-libghostty](https://github.com/behzade/gpui-libghostty)'s binding
pattern, this crate has no GPUI runtime dependency. `bind_gpui!` generates the adapter against the host application's
GPUI crate or re-export, avoiding a second incompatible GPUI version. The adapter
currently targets the GPUI API re-exported by gpui-kit 0.7.3.

```rust,ignore
mod terminal {
    gpui_rio::bind_gpui!(gpui_kit);
}
use gpui_rio::{TerminalColor, TerminalOptions, TerminalStyle, TerminalTheme};

// In an application callback with `cx: &mut App` and `window: &mut Window`:
let background = TerminalColor { r: 20, g: 20, b: 20 };
let foreground = TerminalColor { r: 230, g: 230, b: 230 };
let style = TerminalStyle {
    theme: TerminalTheme::new(background, foreground, [foreground; 16]),
    font_family: "monospace".into(),
    font_size: 13.,
};
let terminal = terminal::Terminal::spawn(
    TerminalOptions::new("/bin/sh", "/tmp", style), cx,
)?;
terminal.read(cx).focus(window, cx);
// Add `terminal` as a child of your view.
```

Supply your own 16-color ANSI palette. `update_style` applies colors and font
settings without restarting the command; notify the containing view afterward.
Font sizes use logical pixels and follow the host's supplied zoom. Missing fonts
and generic monospace aliases fall back to a known installed monospace family.
The host should supply an installed monospace font when choosing an explicit family.

The entity provides `focus`, `set_visible`, and `is_alive`. Hiding it preserves the
shell and scrollback; dropping it asks the PTY worker to terminate and reap the
shell. Clipboard shortcuts are Cmd+C/V on macOS and Ctrl+Shift+C/V on Linux.
The adapter supports text selection, wheel scrollback, ANSI styles, bracketed
paste, and IME composition. It uses the installed `xterm-256color` terminfo.

Host applications own session persistence, drawers, toolbars, shortcuts, and theme
selection. Windows, terminal graphics, and application mouse reporting are not
implemented. No Zig or native terminal application is required.

Run `cargo test -p gpui-rio` for GPUI input, PTY resize, working-directory, and
process cleanup tests. GPUI is a development dependency for these tests only.
