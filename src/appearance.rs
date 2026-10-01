//! Theme catalog and selection adapted from FactorSeal desktop.

use std::sync::LazyLock;

use anyhow::Result;
use gpui_kit::component::theme::Theme;
#[cfg(test)]
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::{App, Global, Hsla, SharedString};
use native_theme_gpui::{AccessibilityPreferences, ColorMode};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Choice {
    #[default]
    #[serde(rename = "agentaps")]
    Agentaps,
    System,
    OneDark,
    Dracula,
    SolarizedLight,
    SolarizedDark,
    Nord,
    CatppuccinMocha,
    GruvboxLight,
    GruvboxDark,
    OneLight,
    DraculaLight,
    NordLight,
    TokyoNight,
    TokyoNightDay,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    themes: Vec<ThemeEntry>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeEntry {
    id: Choice,
    label: String,
    preset: Option<String>,
    mode: Option<Variant>,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Variant {
    Light,
    Dark,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    toml::from_str(include_str!("../themes/catalog.toml")).expect("invalid embedded theme catalog")
});

impl Choice {
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        CATALOG.themes.iter().map(|entry| entry.id)
    }

    fn entry(self) -> &'static ThemeEntry {
        CATALOG
            .themes
            .iter()
            .find(|entry| entry.id == self)
            .expect("theme missing from embedded catalog")
    }

    pub(crate) fn label(self) -> &'static str {
        &self.entry().label
    }

    fn preset(self) -> Option<(&'static str, ColorMode)> {
        let entry = self.entry();
        entry.preset.as_deref().zip(entry.mode).map(|(name, mode)| {
            (
                name,
                match mode {
                    Variant::Light => ColorMode::Light,
                    Variant::Dark => ColorMode::Dark,
                },
            )
        })
    }
}

struct Preferences {
    choice: Choice,
    scale: f32,
    font: Option<SharedString>,
    system_theme: Theme,
    system_input: Hsla,
}

impl Global for Preferences {}

fn resolve(choice: Choice, system: &Theme, system_input: Hsla) -> Result<(Theme, Hsla)> {
    let mut theme = system.clone();
    let input = if let Some((preset, mode)) = choice.preset() {
        let (converted, resolved) = native_theme_gpui::from_preset(
            preset,
            matches!(mode, ColorMode::Dark),
            &AccessibilityPreferences::default(),
        )?;
        theme = converted;
        theme.font_family = system.font_family.clone();
        theme.font_size = system.font_size;
        let color = resolved.input.background_color;
        crate::theming::hsla(gpui_kit::rgba(u32::from_be_bytes([
            color.r, color.g, color.b, color.a,
        ])))
    } else if choice == Choice::Agentaps {
        let mode = system.mode;
        let config = if mode.is_dark() {
            &system.dark_theme
        } else {
            &system.light_theme
        };
        theme.apply_config(config);
        theme.mode = mode;
        crate::theme::brand(&mut theme);
        theme.input_background()
    } else {
        system_input
    };
    theme.highlight_theme = if theme.mode.is_dark() {
        gpui_kit::component::highlighter::HighlightTheme::default_dark()
    } else {
        gpui_kit::component::highlighter::HighlightTheme::default_light()
    };
    theme.switch = theme.muted_foreground;
    theme.switch_thumb = theme.background;
    theme.tokens = (&theme.colors).into();
    Ok((theme, input))
}

pub(crate) fn initialize(cx: &mut App) {
    cx.set_global(Preferences {
        choice: Choice::default(),
        scale: 1.,
        font: None,
        system_theme: Theme::global(cx).clone(),
        system_input: crate::theming::input_background(cx),
    });
}

pub(crate) fn current(cx: &App) -> Choice {
    cx.try_global::<Preferences>()
        .map_or(Choice::default(), |preferences| preferences.choice)
}

pub(crate) fn select(choice: Choice, scale: f32, cx: &mut App) -> Result<()> {
    if !cx.has_global::<Preferences>() {
        initialize(cx);
    }
    let preferences = cx.global::<Preferences>();
    let (mut theme, input) = resolve(choice, &preferences.system_theme, preferences.system_input)?;
    if let Some(font) = &preferences.font {
        theme.font_family = font.clone();
    }
    // Keep zoom independent of the selected palette and native font metrics.
    crate::app::set_theme_font_scale(&mut theme, scale);
    theme.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Always;
    *Theme::global_mut(cx) = theme;
    crate::theming::set_input_background(input, cx);
    Theme::sync_base(cx);
    let preferences = cx.global_mut::<Preferences>();
    preferences.choice = choice;
    preferences.scale = scale;
    cx.refresh_windows();
    Ok(())
}

pub(crate) fn set_font(font: Option<SharedString>, cx: &mut App) {
    if !cx.has_global::<Preferences>() {
        initialize(cx);
    }
    cx.global_mut::<Preferences>().font = font;
}

pub(crate) fn set_scale(scale: f32, cx: &mut App) {
    if cx.has_global::<Preferences>() {
        cx.global_mut::<Preferences>().scale = scale;
    }
}

pub(crate) fn system_changed(cx: &mut App) {
    let theme = Theme::global(cx).clone();
    let input = crate::theming::input_background(cx);
    let preferences = cx.global_mut::<Preferences>();
    preferences.system_theme = theme;
    preferences.system_input = input;
    let choice = preferences.choice;
    let scale = preferences.scale;
    if let Err(error) = select(choice, scale, cx) {
        eprintln!("could not refresh desktop theme: {error:#}");
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[gpui_kit::test]
    fn system_refresh_preserves_the_selected_palette_and_zoom(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            initialize(cx);
            select(Choice::Dracula, 1.4, cx).unwrap();
            for choice in Choice::all() {
                select(choice, 1., cx).unwrap();
                let theme = Theme::global(cx);
                assert_eq!(theme.switch, theme.muted_foreground, "{choice:?}");
                assert_eq!(theme.switch_thumb, theme.background, "{choice:?}");
                assert_eq!(theme.tokens.switch.color, theme.switch, "{choice:?}");
            }
            select(Choice::Dracula, 1.4, cx).unwrap();
            let dracula = Theme::global(cx).background;
            Theme::change(ThemeMode::Light, None, cx);
            let system = Theme::global(cx).background;
            crate::theming::set_input_background(system, cx);
            system_changed(cx);
            assert_eq!(current(cx), Choice::Dracula);
            assert_eq!(Theme::global(cx).background, dracula);
            assert_eq!(Theme::global(cx).font_size, gpui_kit::px(16. * 1.4));
            select(Choice::System, 1.4, cx).unwrap();
            assert_eq!(Theme::global(cx).background, system);
            assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
            assert_eq!(crate::theming::input_background(cx), system);
            assert_eq!(Theme::global(cx).font_size, gpui_kit::px(16. * 1.4));
        });
    }

    #[gpui_kit::test]
    fn agentaps_follows_system_light_and_dark_changes(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            initialize(cx);
            select(Choice::Agentaps, 1.3, cx).unwrap();
            for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Light] {
                Theme::change(mode, None, cx);
                crate::theming::set_input_background(Theme::global(cx).input_background(), cx);
                system_changed(cx);
                let theme = Theme::global(cx);
                let palette = crate::theme::palette(cx);
                assert_eq!(current(cx), Choice::Agentaps);
                assert_eq!(theme.mode, mode);
                assert_eq!(theme.font_size, gpui_kit::px(16. * 1.3));
                assert_eq!(palette.color(crate::theme::BG), theme.background);
                assert_eq!(palette.color(crate::theme::TEXT), theme.foreground);
                assert_eq!(palette.color(crate::theme::SIDEBAR), theme.sidebar);
                assert_eq!(
                    theme.tokens.button_primary.background,
                    theme.button_primary.into()
                );
                if mode.is_dark() {
                    assert!(theme.background.l < theme.foreground.l);
                } else {
                    assert!(theme.background.l > theme.foreground.l);
                }
            }
        });
    }

    #[test]
    fn embedded_catalog_preserves_theme_ids_and_valid_sources() {
        let expected = [
            "agentaps",
            "system",
            "gruvbox-light",
            "gruvbox-dark",
            "one-light",
            "one-dark",
            "dracula-light",
            "dracula",
            "solarized-light",
            "solarized-dark",
            "nord-light",
            "nord",
            "tokyo-night-day",
            "tokyo-night",
            "catppuccin-latte",
            "catppuccin-frappe",
            "catppuccin-macchiato",
            "catppuccin-mocha",
        ];
        let ids: Vec<_> = Choice::all()
            .map(|choice| serde_json::to_value(choice).unwrap())
            .collect();
        assert_eq!(ids, expected.map(serde_json::Value::from));
        for entry in &CATALOG.themes {
            assert!(!entry.label.trim().is_empty());
            let builtin = matches!(entry.id, Choice::Agentaps | Choice::System);
            assert_eq!(entry.preset.is_none(), builtin);
            assert_eq!(entry.mode.is_none(), builtin);
        }
    }

    #[test]
    fn additional_bundled_variants_have_distinct_palettes() {
        for preset in ["gruvbox", "one-dark", "dracula", "nord", "tokyo-night"] {
            let theme = native_theme_gpui::Theme::preset(preset).unwrap();
            let light = theme
                .clone()
                .into_variant(ColorMode::Light)
                .unwrap()
                .resolve_system()
                .unwrap();
            let dark = theme
                .into_variant(ColorMode::Dark)
                .unwrap()
                .resolve_system()
                .unwrap();
            assert_ne!(
                light.window.background_color, dark.window.background_color,
                "{preset}"
            );
            assert_ne!(
                light.defaults.text_color, dark.defaults.text_color,
                "{preset}"
            );
        }
        for (preset, mode) in [
            ("catppuccin-latte", ColorMode::Light),
            ("catppuccin-frappe", ColorMode::Dark),
            ("catppuccin-macchiato", ColorMode::Dark),
        ] {
            let theme = native_theme_gpui::Theme::preset(preset)
                .unwrap()
                .into_variant(mode)
                .unwrap()
                .resolve_system()
                .unwrap();
            assert_ne!(
                theme.window.background_color, theme.defaults.text_color,
                "{preset}"
            );
        }
    }

    #[test]
    fn every_theme_resolves_and_system_colors_are_restored() {
        let system = Theme {
            colors: gpui_kit::component::ThemeColor {
                background: crate::theming::hsla(gpui_kit::rgb(0x0012_3456)),
                ..Theme::default().colors
            },
            ..Theme::default()
        };
        let input = crate::theming::hsla(gpui_kit::rgb(0x0065_4321));
        for choice in Choice::all() {
            let (theme, _) = resolve(choice, &system, input).unwrap();
            assert_eq!(
                theme.tokens.button_primary.background,
                theme.button_primary.into()
            );
            let (restored, restored_input) = resolve(Choice::System, &system, input).unwrap();
            assert_eq!(restored.background, system.background);
            assert_eq!(restored_input, input);
        }
    }
}
