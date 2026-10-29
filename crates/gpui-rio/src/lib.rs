//! Rio VT parsing and PTY management with a GPUI canvas adapter.
//! Bind to your application's GPUI version with [`bind_gpui!`].
#![cfg(any(target_os = "linux", target_os = "macos"))]

mod adapter;
mod options;
pub use options::{TerminalColor, TerminalOptions, TerminalStyle, TerminalTheme};

#[doc(hidden)]
pub mod __private {
    pub use anyhow;
    pub use rio_vt;
}

#[cfg(test)]
pub mod test_binding {
    crate::__bind_gpui!(
        gpui_kit,
        mod tests {
            include!("tests.rs");
        }
    );
}
