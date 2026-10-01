//! Agentaps palette and semantic colors for custom views.

#[cfg(test)]
use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, rgb};

#[cfg(test)]
pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    brand(Theme::global_mut(cx));
    Theme::sync_base(cx);
    crate::appearance::initialize(cx);
    Theme::set_scrollbar_mode(ScrollbarMode::Always, cx);
}

pub const BG: u32 = 0x10151c;
pub const SIDEBAR: u32 = 0x151c25;
pub const SURFACE: u32 = 0x202a36;
pub const BORDER: u32 = 0x334253;
pub const TEXT: u32 = 0xedf2f7;
pub const MUTED: u32 = 0x9aaaba;
pub const ACCENT: u32 = 0x8fc5ec;

pub const HOVER: u32 = 0x293747;
pub const SELECTED: u32 = 0x30485f;
pub const DROP_TARGET: u32 = 0x395871;
pub const USER_BUBBLE: u32 = 0x29465c;
pub const AGENT_BUBBLE: u32 = 0x202e3b;
pub const ACCENT_SURFACE: u32 = 0x304b60;

pub const STATUS_CONNECTING: u32 = 0xa2b0be;
pub const STATUS_IDLE: u32 = 0x8aa9c0;
pub const STATUS_WORKING: u32 = 0xe9b978;
pub const STATUS_DONE: u32 = 0x4ade80;
pub const STATUS_ERROR: u32 = 0xe99191;
pub const STATUS_QUESTION: u32 = 0xc8a7f4;

pub const TOOL_MARKER: u32 = STATUS_WORKING;
pub const PERMISSION_BORDER: u32 = 0xc99463;
pub const ERROR_SURFACE: u32 = 0x51343a;
pub const ERROR_TEXT: u32 = 0xffc9c9;

// Original palette values remain stable keys for existing status and activity roles.
// Custom views take a snapshot each render so hover callbacks use the same palette.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    colors: gpui_kit::component::theme::ThemeColor,
    branded: bool,
    mode: ThemeMode,
    pub(crate) input: Hsla,
}

pub(crate) fn palette(cx: &App) -> Palette {
    Palette {
        colors: Theme::global(cx).colors,
        mode: Theme::global(cx).mode,
        input: crate::theming::input_background(cx),
        branded: crate::appearance::current(cx) == crate::appearance::Choice::Agentaps,
    }
}

impl Palette {
    pub(crate) fn color(self, role: u32) -> Hsla {
        if self.branded {
            return brand_color(role, self.mode);
        }
        let c = self.colors;
        match role {
            BG => c.background,
            SIDEBAR => c.sidebar,
            SURFACE | AGENT_BUBBLE => c.muted,
            BORDER => c.border,
            TEXT => c.foreground,
            MUTED | STATUS_CONNECTING | STATUS_IDLE => c.muted_foreground,
            ACCENT => c.link,
            HOVER => c.list_hover,
            SELECTED | DROP_TARGET => c.list_active,
            USER_BUBBLE | ACCENT_SURFACE => c.selection.opacity(0.25),
            STATUS_WORKING | PERMISSION_BORDER => c.warning,
            STATUS_DONE => c.success,
            STATUS_ERROR | ERROR_TEXT => c.danger,
            STATUS_QUESTION => c.primary,
            ERROR_SURFACE => c.danger.opacity(0.15),
            crate::diff_view::ADDED_TEXT => c.success,
            crate::diff_view::REMOVED_TEXT => c.danger,
            crate::diff_view::ADDED_BG => c.success.opacity(0.12),
            crate::diff_view::REMOVED_BG => c.danger.opacity(0.12),
            _ => rgb(role).into(),
        }
    }
}

fn brand_color(role: u32, mode: ThemeMode) -> Hsla {
    let value = if mode.is_dark() {
        role
    } else {
        match role {
            BG => 0xf8fafc,
            SIDEBAR => 0xf0f4f8,
            SURFACE => 0xffffff,
            BORDER => 0xccd6e0,
            TEXT => 0x1d2a38,
            MUTED => 0x526579,
            ACCENT => 0x206b9f,
            HOVER => 0xe4ecf4,
            SELECTED => 0xd3e5f2,
            DROP_TARGET => 0xc4ddeb,
            USER_BUBBLE => 0xdcecf8,
            AGENT_BUBBLE => 0xedf2f7,
            ACCENT_SURFACE => 0xe0edf7,
            STATUS_CONNECTING => 0x64748b,
            STATUS_IDLE => 0x50738b,
            STATUS_WORKING => 0x9a640e,
            STATUS_DONE => 0x15803d,
            STATUS_ERROR => 0xb33e45,
            STATUS_QUESTION => 0x7951a9,
            PERMISSION_BORDER => 0xa96a25,
            ERROR_SURFACE => 0xfce8ea,
            ERROR_TEXT => 0x9f2933,
            crate::diff_view::ADDED_BG => 0xe4f3ea,
            crate::diff_view::REMOVED_BG => 0xfde9e9,
            crate::diff_view::ADDED_TEXT => 0x217447,
            crate::diff_view::REMOVED_TEXT => 0xab3434,
            _ => role,
        }
    };
    rgb(value).into()
}

pub(crate) fn brand(theme: &mut Theme) {
    let mode = theme.mode;
    let color = |role| brand_color(role, mode);
    theme.background = color(BG);
    theme.foreground = color(TEXT);
    theme.sidebar = color(SIDEBAR);
    theme.sidebar_foreground = theme.foreground;
    theme.sidebar_border = color(BORDER);
    theme.sidebar_accent = color(HOVER);
    theme.sidebar_accent_foreground = theme.foreground;
    theme.sidebar_primary = color(SELECTED);
    theme.sidebar_primary_foreground = theme.foreground;
    theme.border = color(BORDER);
    theme.input = theme.border;
    theme.muted = color(SURFACE);
    theme.muted_foreground = color(MUTED);
    theme.secondary = theme.muted;
    theme.secondary_foreground = theme.foreground;
    theme.secondary_hover = color(HOVER);
    theme.secondary_active = color(SELECTED);
    theme.primary = color(ACCENT);
    theme.primary_foreground = color(BG);
    theme.primary_hover = theme.primary;
    theme.primary_active = theme.primary;
    theme.accent = color(SELECTED);
    theme.accent_foreground = theme.foreground;
    theme.popover = color(SURFACE);
    theme.popover_foreground = theme.foreground;
    theme.list_hover = color(HOVER);
    theme.list_active = color(SELECTED);
    theme.list_active_border = theme.border;
    theme.selection = color(SELECTED);
    theme.ring = theme.primary;
    theme.caret = theme.foreground;
    theme.link = theme.primary;
    theme.link_hover = theme.primary;
    theme.link_active = theme.primary;
    theme.success = color(STATUS_DONE);
    theme.success_foreground = color(BG);
    theme.success_hover = theme.success;
    theme.success_active = theme.success;
    theme.warning = color(STATUS_WORKING);
    theme.warning_foreground = color(BG);
    theme.danger = color(STATUS_ERROR);
    theme.danger_foreground = color(BG);
    theme.danger_hover = theme.danger;
    theme.danger_active = theme.danger;
    theme.info = theme.primary;
    theme.info_foreground = color(BG);
    theme.progress_bar = theme.primary;
    theme.title_bar = theme.background;
    theme.title_bar_border = theme.border;
    theme.window_border = theme.border;
    sync_brand_controls(theme);
}

fn sync_brand_controls(theme: &mut gpui_kit::component::theme::Theme) {
    theme.tab = theme.background;
    theme.tab_foreground = theme.muted_foreground;
    theme.tab_active = theme.popover;
    theme.tab_active_foreground = theme.foreground;
    theme.tab_bar = theme.background;
    theme.button = theme.secondary;
    theme.button_foreground = theme.secondary_foreground;
    theme.button_hover = theme.secondary_hover;
    theme.button_active = theme.secondary_active;
    theme.button_primary = theme.primary;
    theme.button_primary_foreground = theme.primary_foreground;
    theme.button_primary_hover = theme.primary_hover;
    theme.button_primary_active = theme.primary_active;
    theme.button_secondary = theme.secondary;
    theme.button_secondary_foreground = theme.secondary_foreground;
    theme.button_secondary_hover = theme.secondary_hover;
    theme.button_secondary_active = theme.secondary_active;
    theme.button_danger = theme.danger;
    theme.button_danger_foreground = theme.danger_foreground;
    theme.button_danger_hover = theme.danger_hover;
    theme.button_danger_active = theme.danger_active;
    theme.button_success = theme.success;
    theme.button_success_foreground = theme.success_foreground;
    theme.button_success_hover = theme.success_hover;
    theme.button_success_active = theme.success_active;
    theme.button_info = theme.info;
    theme.button_info_foreground = theme.info_foreground;
    theme.button_info_hover = theme.info;
    theme.button_info_active = theme.info;
    theme.button_warning = theme.warning;
    theme.button_warning_foreground = theme.warning_foreground;
    theme.button_warning_hover = theme.warning;
    theme.button_warning_active = theme.warning;
    theme.tokens = (&theme.colors).into();
}
