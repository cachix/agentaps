use rio_vt::config::colors::ColorRgb;

pub type TerminalColor = ColorRgb;

#[derive(Clone, Copy, PartialEq)]
pub struct TerminalTheme {
    pub background: ColorRgb,
    pub foreground: ColorRgb,
    ansi: [ColorRgb; 16],
}

impl TerminalTheme {
    pub fn new(background: ColorRgb, foreground: ColorRgb, ansi: [ColorRgb; 16]) -> Self {
        Self {
            background,
            foreground,
            ansi,
        }
    }

    #[doc(hidden)]
    pub fn color(self, index: usize) -> ColorRgb {
        match index {
            0..=15 => self.ansi[index],
            16..=231 => {
                let n = index - 16;
                let channel = |n: usize| if n == 0 { 0 } else { (55 + n * 40) as u8 };
                ColorRgb {
                    r: channel(n / 36),
                    g: channel(n / 6 % 6),
                    b: channel(n % 6),
                }
            }
            232..=255 => {
                let v = (8 + (index - 232) * 10) as u8;
                ColorRgb { r: v, g: v, b: v }
            }
            257 => self.background,
            259..=266 => self.ansi[index - 259] * 0.66,
            268 => self.foreground * 0.66,
            _ => self.foreground,
        }
    }
}

/// Appearance supplied by the host application, in logical pixels.
#[derive(Clone, PartialEq)]
pub struct TerminalStyle {
    pub theme: TerminalTheme,
    /// An installed monospace family, or "monospace" for platform fallback.
    pub font_family: String,
    pub font_size: f32,
}

/// Command and appearance for a new terminal. The host controls initial focus.
pub struct TerminalOptions {
    pub command: String,
    pub working_directory: std::path::PathBuf,
    pub style: TerminalStyle,
}

impl TerminalOptions {
    pub fn new(
        command: impl Into<String>,
        working_directory: impl Into<std::path::PathBuf>,
        style: TerminalStyle,
    ) -> Self {
        Self {
            command: command.into(),
            working_directory: working_directory.into(),
            style,
        }
    }
}
