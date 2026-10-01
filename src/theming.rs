//! GTK/Qt probes, palette mapping, and live monitoring ported from FactorSeal desktop.

use std::fmt;

use gpui_kit::{App, Global};

pub(crate) fn hsla(color: gpui_kit::Rgba) -> gpui_kit::Hsla {
    color.into()
}

#[cfg(target_os = "linux")]
use std::{
    env, io,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[cfg(target_os = "linux")]
use anyhow::{Context as _, Result, bail};
#[cfg(target_os = "linux")]
use detect_desktop_environment::DesktopEnvironment;
#[cfg(target_os = "linux")]
use futures_util::{FutureExt as _, StreamExt as _};
#[cfg(target_os = "linux")]
use gpui_kit::Task;
#[cfg(target_os = "linux")]
use notify::Watcher as _;

/// Native toolkit used to resolve the current desktop theme.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) enum Backend {
    Gtk,
    Qt,
}

impl Backend {
    #[cfg(target_os = "linux")]
    const fn probe_argument(self) -> &'static str {
        match self {
            Self::Gtk => "--gtk-theme-probe",
            Self::Qt => "--qt-theme-probe",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Gtk => "GTK",
            Self::Qt => "Qt",
        })
    }
}

/// Theme status used by the automatic native-theme monitor.
#[derive(Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct ThemeState {
    pub(crate) backend: Option<Backend>,
    pub(crate) summary: String,
    pub(crate) error: Option<String>,
    pub(crate) monitor_status: String,
    pub(crate) automatic_refreshes: u64,
    pub(crate) last_change_source: Option<&'static str>,
}

impl Global for ThemeState {}

struct InputBackground(gpui_kit::Hsla);

impl Global for InputBackground {}

pub(crate) fn set_input_background(color: gpui_kit::Hsla, cx: &mut App) {
    cx.set_global(InputBackground(color));
}

pub(crate) fn input_background(cx: &App) -> gpui_kit::Hsla {
    cx.try_global::<InputBackground>().map_or_else(
        || gpui_kit::component::theme::Theme::global(cx).input_background(),
        |background| background.0,
    )
}

#[cfg(target_os = "linux")]
struct LoadedTheme {
    backend: Backend,
    summary: String,
    mode: gpui_kit::component::ThemeMode,
    resolved: native_theme_gpui::ResolvedTheme,
}

#[cfg(target_os = "linux")]
struct ThemeMonitor {
    _file_watcher: Option<notify::RecommendedWatcher>,
    _portal_task: Task<()>,
    _refresh_task: Task<()>,
}

#[cfg(target_os = "linux")]
impl Global for ThemeMonitor {}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangeSource {
    Portal,
    Files,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ChangeSources {
    portal: bool,
    files: bool,
}

#[cfg(target_os = "linux")]
impl ChangeSources {
    fn add(&mut self, source: ChangeSource) {
        match source {
            ChangeSource::Portal => self.portal = true,
            ChangeSource::Files => self.files = true,
        }
    }

    const fn label(self) -> &'static str {
        match (self.portal, self.files) {
            (true, true) => "desktop portal and theme files",
            (true, false) => "desktop portal",
            (false, true) => "theme files",
            (false, false) => "theme monitor",
        }
    }
}

/// Load the native theme and start monitoring it for changes.
pub(crate) fn initialize(cx: &mut App) {
    gpui_kit::component::theme::Theme::sync_system_appearance(None, cx);
    #[cfg(target_os = "linux")]
    initialize_linux(cx);

    #[cfg(not(target_os = "linux"))]
    cx.set_global(ThemeState {
        backend: None,
        summary: "Using gpui-component's platform default theme".to_owned(),
        error: None,
        monitor_status: "native GTK/Qt monitoring is only enabled on Linux".to_owned(),
        automatic_refreshes: 0,
        last_change_source: None,
    });
}

#[cfg(target_os = "linux")]
fn initialize_linux(cx: &mut App) {
    let initial = load_automatic();
    let state = match initial {
        Ok(loaded) => {
            install_theme(&loaded, cx);
            ThemeState {
                backend: Some(loaded.backend),
                summary: loaded.summary,
                error: None,
                monitor_status: "starting".to_owned(),
                automatic_refreshes: 0,
                last_change_source: None,
            }
        }
        Err(error) => ThemeState {
            backend: None,
            summary: "Using gpui-component's default theme".to_owned(),
            error: Some(format!("Native theme detection failed: {error:#}")),
            monitor_status: "starting".to_owned(),
            automatic_refreshes: 0,
            last_change_source: None,
        },
    };
    cx.set_global(state);
    setup_theme_monitor(cx);
}

/// Reload the native theme and repaint all GPUI windows.
#[cfg(target_os = "linux")]
fn refresh_native_theme(cx: &mut App) {
    match load_automatic() {
        Ok(loaded) => {
            install_theme(&loaded, cx);
            let backend = loaded.backend;
            let summary = loaded.summary;
            eprintln!("active theme backend is {backend}: {summary}");
            let state = cx.global_mut::<ThemeState>();
            state.backend = Some(backend);
            state.summary = summary;
            state.error = None;
            crate::appearance::system_changed(cx);
        }
        Err(error) => {
            let state = cx.global_mut::<ThemeState>();
            state.error = Some(format!("Theme refresh failed: {error:#}"));
        }
    }
    cx.refresh_windows();
}

#[cfg(target_os = "linux")]
fn add_config_candidates(paths: &mut Vec<PathBuf>, root: &Path) {
    for name in [
        "gtk-3.0",
        "gtk-4.0",
        "qt5ct",
        "qt6ct",
        "Kvantum",
        "kdeglobals",
        "Trolltech.conf",
    ] {
        paths.push(root.join(name));
    }
}

#[cfg(target_os = "linux")]
fn add_data_candidates(paths: &mut Vec<PathBuf>, root: &Path) {
    paths.push(root.join("themes"));
    paths.push(root.join("color-schemes"));
}

#[cfg(target_os = "linux")]
fn candidate_theme_paths(
    home: Option<&Path>,
    config_home: Option<&Path>,
    data_home: Option<&Path>,
    config_dirs: &[PathBuf],
    data_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_home) = config_home {
        add_config_candidates(&mut paths, config_home);
    }
    if let Some(data_home) = data_home {
        add_data_candidates(&mut paths, data_home);
    }
    if let Some(home) = home {
        paths.push(home.join(".themes"));
    }

    for root in config_dirs {
        add_config_candidates(&mut paths, root);
    }
    for root in data_dirs {
        add_data_candidates(&mut paths, root);
    }

    paths.sort_unstable();
    paths.dedup();
    paths
}

#[cfg(target_os = "linux")]
fn theme_watch_paths() -> Vec<PathBuf> {
    let xdg_directories = xdg::BaseDirectories::new();
    let home = env::var_os("HOME").map(PathBuf::from);
    let config_home = xdg_directories.get_config_home();
    let data_home = xdg_directories.get_data_home();
    let config_dirs = xdg_directories.get_config_dirs();
    let data_dirs = xdg_directories.get_data_dirs();
    let mut paths = candidate_theme_paths(
        home.as_deref(),
        config_home.as_deref(),
        data_home.as_deref(),
        &config_dirs,
        &data_dirs,
    );

    if let Some(extra) = env::var_os("NATIVE_THEME_WATCH_PATH") {
        paths.extend(env::split_paths(&extra));
    }
    paths.retain(|path| path.exists());
    paths.sort_unstable();
    paths.dedup();
    paths
}

#[cfg(target_os = "linux")]
fn is_theme_setting(namespace: &str, key: &str) -> bool {
    if namespace == ashpd::desktop::settings::APPEARANCE_NAMESPACE {
        return true;
    }

    let namespace = namespace.to_ascii_lowercase();
    let key = key.to_ascii_lowercase();
    namespace.contains("interface")
        && ["theme", "color", "contrast", "font", "icon", "cursor"]
            .iter()
            .any(|part| key.contains(part))
}

#[cfg(target_os = "linux")]
fn create_file_watcher(
    paths: &[PathBuf],
    sender: smol::channel::Sender<ChangeSource>,
) -> Result<(notify::RecommendedWatcher, usize)> {
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            return;
        };
        if matches!(
            event.kind,
            notify::EventKind::Create(_)
                | notify::EventKind::Modify(_)
                | notify::EventKind::Remove(_)
        ) {
            let _ = sender.try_send(ChangeSource::Files);
        }
    })?;

    let mut watched_path_count = 0;
    for path in paths {
        let mode = if path.is_dir() {
            notify::RecursiveMode::Recursive
        } else {
            notify::RecursiveMode::NonRecursive
        };
        match watcher.watch(path, mode) {
            Ok(()) => watched_path_count += 1,
            Err(error) => eprintln!("could not watch {}: {error}", path.display()),
        }
    }
    if watched_path_count == 0 {
        bail!("no GTK or Qt theme locations were available to watch");
    }
    Ok((watcher, watched_path_count))
}

#[cfg(target_os = "linux")]
async fn watch_portal(
    sender: smol::channel::Sender<ChangeSource>,
    executor: gpui_kit::BackgroundExecutor,
) {
    const RETRY_DELAY: Duration = Duration::from_secs(5);

    loop {
        match ashpd::desktop::settings::Settings::new().await {
            Ok(settings) => match settings.receive_setting_changed().await {
                Ok(mut changes) => {
                    while let Some(setting) = changes.next().await {
                        if is_theme_setting(setting.namespace(), setting.key()) {
                            let _ = sender.try_send(ChangeSource::Portal);
                        }
                    }
                    eprintln!("desktop settings portal stream ended; reconnecting");
                }
                Err(error) => {
                    eprintln!("could not subscribe to desktop theme changes: {error}");
                }
            },
            Err(error) => eprintln!("could not connect to desktop settings portal: {error}"),
        }
        executor.timer(RETRY_DELAY).await;
    }
}

#[cfg(target_os = "linux")]
fn setup_theme_monitor(cx: &mut App) {
    const DEBOUNCE: Duration = Duration::from_millis(750);
    const CHANNEL_CAPACITY: usize = 32;

    let (sender, receiver) = smol::channel::bounded(CHANNEL_CAPACITY);
    let paths = theme_watch_paths();
    let (file_watcher, watched_paths) = match create_file_watcher(&paths, sender.clone()) {
        Ok((watcher, count)) => (Some(watcher), count),
        Err(error) => {
            eprintln!("theme file monitoring unavailable: {error}");
            (None, 0)
        }
    };

    let executor = cx.background_executor().clone();
    let portal_task = executor.spawn(watch_portal(sender, executor.clone()));
    let refresh_task = cx.spawn(async move |cx| {
        'events: while let Ok(source) = receiver.recv().await {
            let mut sources = ChangeSources::default();
            sources.add(source);

            loop {
                let next_event = receiver.recv().fuse();
                let quiet_period = cx.background_executor().timer(DEBOUNCE).fuse();
                futures_util::pin_mut!(next_event, quiet_period);
                futures_util::select_biased! {
                    next = next_event => match next {
                        Ok(source) => sources.add(source),
                        Err(_) => break 'events,
                    },
                    () = quiet_period => break,
                }
            }

            cx.update(|cx| {
                refresh_native_theme(cx);
                let state = cx.global_mut::<ThemeState>();
                state.automatic_refreshes += 1;
                state.last_change_source = Some(sources.label());
                cx.refresh_windows();
                eprintln!("automatic theme refresh from {}", sources.label());
            });
        }
    });

    cx.global_mut::<ThemeState>().monitor_status =
        format!("XDG portal and {watched_paths} GTK/Qt filesystem locations");
    cx.set_global(ThemeMonitor {
        _file_watcher: file_watcher,
        _portal_task: portal_task,
        _refresh_task: refresh_task,
    });
}

#[cfg(target_os = "linux")]
fn preference_from(
    explicit: Option<&str>,
    desktop: Option<DesktopEnvironment>,
    kde_full_session: bool,
) -> [Backend; 2] {
    match explicit
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("gtk") => return [Backend::Gtk, Backend::Qt],
        Some("qt") => return [Backend::Qt, Backend::Gtk],
        _ => {}
    }

    let qt_desktop = desktop.is_some_and(DesktopEnvironment::qt);
    if qt_desktop || kde_full_session {
        [Backend::Qt, Backend::Gtk]
    } else {
        [Backend::Gtk, Backend::Qt]
    }
}

#[cfg(target_os = "linux")]
fn preferred_backends() -> [Backend; 2] {
    let explicit = env::var("NATIVE_THEME_BACKEND").ok();
    preference_from(
        explicit.as_deref(),
        DesktopEnvironment::detect(),
        env::var_os("KDE_FULL_SESSION").is_some(),
    )
}

#[cfg(target_os = "linux")]
fn run_probe(backend: Backend) -> Result<Vec<u8>> {
    let executable = env::current_exe().context("could not locate agentaps")?;
    let output = Command::new(executable)
        .arg(backend.probe_argument())
        .output()
        .with_context(|| format!("failed to start the {backend} theme probe"))?;

    if !output.status.success() {
        bail!(
            "{backend} theme probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

#[cfg(target_os = "linux")]
fn probe_is_dark(channels: [f32; 3]) -> bool {
    let [red, green, blue] = channels.map(|channel| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    // Match the GTK/Qt bridges' luminance threshold.
    0.2126 * red + 0.7152 * green + 0.0722 * blue < 0.5
}

/// The probe crates use native-theme 0.5.7. Transfer their sparse theme data
/// through its serde representation and resolve against the connector's model.
/// Seeding the current preset supplies fields added after the probe crates shipped.
#[cfg(target_os = "linux")]
fn resolve_probe(
    preset: &str,
    mode: native_theme_gpui::ColorMode,
    probe: &impl serde::Serialize,
) -> Result<native_theme_gpui::ResolvedTheme> {
    let overlay: native_theme_gpui::ThemeMode =
        serde_json::from_value(serde_json::to_value(probe)?)?;
    let mut variant = native_theme_gpui::Theme::preset(preset)?.into_variant(mode)?;
    variant.merge(&overlay);
    Ok(variant.resolve_system()?)
}

#[cfg(target_os = "linux")]
fn load_backend(backend: Backend) -> Result<LoadedTheme> {
    let json = run_probe(backend)?;

    match backend {
        Backend::Gtk => {
            let snapshot: native_theme_gtk::ThemeSnapshot =
                serde_json::from_slice(&json).context("GTK probe returned invalid JSON")?;
            snapshot.validate()?;
            let legacy = snapshot.to_native_theme_mode();
            let background = snapshot
                .semantic_palette
                .window_background
                .context("GTK snapshot is missing window_background")?;
            let mode = if probe_is_dark([background.red, background.green, background.blue]) {
                native_theme_gpui::ColorMode::Dark
            } else {
                native_theme_gpui::ColorMode::Light
            };
            let mut resolved = resolve_probe("adwaita", mode, &legacy)?;
            resolved.popover.background_color = resolved.tooltip.background_color;
            resolved.popover.font = resolved.tooltip.font.clone();
            resolved.sidebar.background_color = resolved.defaults.surface_color;
            resolved.sidebar.font.color = resolved.defaults.text_color;
            resolved.sidebar.border.color = resolved.defaults.border.color;
            resolved.sidebar.hover_background = resolved.defaults.selection_background;
            resolved.sidebar.selection_background = resolved.defaults.selection_background;
            resolved.sidebar.selection_text_color = resolved.defaults.selection_text_color;
            let mode_label = if matches!(mode, native_theme_gpui::ColorMode::Dark) {
                "dark"
            } else {
                "light"
            };
            let summary = format!(
                "GTK theme: {} · GTK {} · {mode_label}",
                snapshot.identity.theme_name, snapshot.identity.toolkit_version
            );
            Ok(LoadedTheme {
                backend,
                summary,
                resolved,
                mode: if matches!(mode, native_theme_gpui::ColorMode::Dark) {
                    gpui_kit::component::ThemeMode::Dark
                } else {
                    gpui_kit::component::ThemeMode::Light
                },
            })
        }
        Backend::Qt => {
            let snapshot: native_theme_qt::ThemeSnapshot =
                serde_json::from_slice(&json).context("Qt probe returned invalid JSON")?;
            snapshot.validate()?;
            let legacy = snapshot.to_native_theme_mode();
            let background = snapshot
                .semantic_palette
                .window_background
                .context("Qt snapshot is missing window_background")?;
            let mode = if probe_is_dark([background.red, background.green, background.blue]) {
                native_theme_gpui::ColorMode::Dark
            } else {
                native_theme_gpui::ColorMode::Light
            };
            let resolved = resolve_probe("kde-breeze", mode, &legacy)?;
            let mode_label = if matches!(mode, native_theme_gpui::ColorMode::Dark) {
                "dark"
            } else {
                "light"
            };
            let summary = format!(
                "Qt style: {} · Qt {} · {mode_label}",
                snapshot.identity.style_name, snapshot.identity.toolkit_version
            );
            Ok(LoadedTheme {
                backend,
                summary,
                resolved,
                mode: if matches!(mode, native_theme_gpui::ColorMode::Dark) {
                    gpui_kit::component::ThemeMode::Dark
                } else {
                    gpui_kit::component::ThemeMode::Light
                },
            })
        }
    }
}

#[cfg(target_os = "linux")]
fn load_automatic() -> Result<LoadedTheme> {
    let [preferred, fallback] = preferred_backends();
    match load_backend(preferred) {
        Ok(theme) => Ok(theme),
        Err(preferred_error) => load_backend(fallback)
            .with_context(|| format!("preferred {preferred} backend failed: {preferred_error:#}")),
    }
}

#[cfg(target_os = "linux")]
fn install_theme(loaded: &LoadedTheme, cx: &mut App) {
    use gpui_kit::component::theme::Theme;

    Theme::change(loaded.mode, None, cx);
    let theme = native_theme_gpui::to_theme(
        &loaded.resolved,
        &loaded.summary,
        loaded.mode.is_dark(),
        &native_theme_gpui::AccessibilityPreferences::default(),
    );
    *Theme::global_mut(cx) = theme;
    let background = loaded.resolved.input.background_color;
    cx.set_global(InputBackground(crate::theming::hsla(gpui_kit::rgba(
        u32::from_be_bytes([background.r, background.g, background.b, background.a]),
    ))));
    Theme::sync_base(cx);
}

/// Emit a toolkit snapshot for the parent process, then terminate.
#[cfg(target_os = "linux")]
fn emit_probe(backend: Backend) -> Result<()> {
    match backend {
        Backend::Gtk => serde_json::to_writer(io::stdout(), &native_theme_gtk::probe()?)?,
        Backend::Qt => serde_json::to_writer(io::stdout(), &native_theme_qt::probe()?)?,
    }
    Ok(())
}

/// Emit a toolkit snapshot for the parent process, then terminate.
#[cfg(target_os = "linux")]
pub(crate) fn exit_after_probe(backend: Backend) -> ! {
    match emit_probe(backend) {
        Ok(()) => {
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{path::Path, thread, time::Duration};

    use super::{
        Backend, ChangeSource, ChangeSources, DesktopEnvironment, candidate_theme_paths,
        create_file_watcher, is_theme_setting, preference_from,
    };

    #[test]
    fn legacy_gtk_and_qt_overrides_reach_the_upstream_connector() {
        use native_theme_gpui::{AccessibilityPreferences, ColorMode};
        let background = serde_json::json!({"red": 0.1, "green": 0.2, "blue": 0.3, "alpha": 1.0});
        let foreground = serde_json::json!({"red": 0.9, "green": 0.8, "blue": 0.7, "alpha": 1.0});
        let palette =
            serde_json::json!({"window_background": background, "window_text": foreground});
        let gtk: native_theme_gtk::ThemeSnapshot = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "backend": "gtk4",
            "identity": {"theme_name": "Test GTK", "toolkit_version": "4.10"},
            "settings": {}, "named_colors": [], "semantic_palette": palette,
            "widgets": [], "diagnostics": []
        }))
        .unwrap();
        let font = serde_json::json!({"family": "Sans", "point_size": 12.0, "pixel_size": -1,
            "weight": 400, "italic": false, "stretch": 100});
        let qt: native_theme_qt::ThemeSnapshot = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "backend": "qt6",
            "identity": {"style_name": "Test Qt", "toolkit_version": "6.2", "platform_backend": "offscreen"},
            "settings": {"font": font, "fixed_font": font, "icon_theme_name": "hicolor",
                "cursor_flash_time_ms": 1200, "double_click_interval_ms": 400,
                "wheel_scroll_lines": 3, "application_stylesheet": ""},
            "palette": {}, "semantic_palette": palette, "controls": {}, "metrics": {}, "hints": {}, "diagnostics": []
        })).unwrap();
        for (preset, overlay) in [
            (
                "adwaita",
                serde_json::to_value(gtk.to_native_theme_mode()).unwrap(),
            ),
            (
                "kde-breeze",
                serde_json::to_value(qt.to_native_theme_mode()).unwrap(),
            ),
        ] {
            for mode in [ColorMode::Light, ColorMode::Dark] {
                let resolved = super::resolve_probe(preset, mode, &overlay).unwrap();
                assert_eq!(resolved.window.background_color.r, 26, "{preset}");
                assert_eq!(resolved.window.background_color.g, 51, "{preset}");
                assert_eq!(resolved.defaults.text_color.r, 230, "{preset}");
                let theme = native_theme_gpui::to_theme(
                    &resolved,
                    preset,
                    matches!(mode, ColorMode::Dark),
                    &AccessibilityPreferences::default(),
                );
                assert_eq!(
                    theme.background,
                    super::hsla(gpui_kit::rgba(u32::from_be_bytes([
                        resolved.window.background_color.r,
                        resolved.window.background_color.g,
                        resolved.window.background_color.b,
                        resolved.window.background_color.a,
                    ])))
                );
                assert_eq!(
                    theme.tokens.button_primary.background,
                    theme.button_primary.into()
                );
            }
        }
    }

    #[test]
    fn probe_mode_matches_the_toolkits_luminance_threshold() {
        assert!(super::probe_is_dark([0., 0., 0.]));
        assert!(!super::probe_is_dark([1., 1., 1.]));
        assert!(super::probe_is_dark([0.7, 0.7, 0.7]));
        assert!(!super::probe_is_dark([0.75, 0.75, 0.75]));
    }

    #[test]
    fn native_palette_and_typography_survive_repeated_theme_changes() {
        use native_theme_gpui::{AccessibilityPreferences, ColorMode, Theme};

        for preset in ["adwaita", "kde-breeze", "adwaita"] {
            for mode in [ColorMode::Light, ColorMode::Dark] {
                let native = Theme::preset(preset)
                    .unwrap()
                    .resolve(mode)
                    .unwrap()
                    .variant;
                let target = native_theme_gpui::to_theme(
                    &native,
                    preset,
                    matches!(mode, ColorMode::Dark),
                    &AccessibilityPreferences::default(),
                );
                let background = native.window.background_color;
                let expected = crate::theming::hsla(gpui_kit::rgba(u32::from_be_bytes([
                    background.r,
                    background.g,
                    background.b,
                    background.a,
                ])));
                assert_eq!(target.background, expected);
                assert_eq!(target.button_primary, target.primary);
                assert_eq!(target.button_primary_foreground, target.primary_foreground);
                assert_eq!(
                    target.tokens.button_primary.background,
                    target.primary.into()
                );
                assert_eq!(target.tokens.popover.background, target.popover.into());
                assert_eq!(
                    target.tokens.scrollbar_thumb.background,
                    target.scrollbar_thumb.into()
                );
                assert_eq!(
                    target.font_family.as_ref(),
                    native.defaults.font.family.as_ref()
                );
                assert_eq!(target.font_size, gpui_kit::px(native.defaults.font.size));
                assert_eq!(
                    target.radius,
                    gpui_kit::px(native.defaults.border.corner_radius)
                );
            }
        }
    }

    #[test]
    fn explicit_backend_wins() {
        assert_eq!(
            preference_from(Some("gtk"), Some(DesktopEnvironment::Kde), true),
            [Backend::Gtk, Backend::Qt]
        );
        assert_eq!(
            preference_from(Some("QT"), Some(DesktopEnvironment::Gnome), false),
            [Backend::Qt, Backend::Gtk]
        );
    }

    #[test]
    fn kde_and_lxqt_prefer_qt() {
        assert_eq!(
            preference_from(None, Some(DesktopEnvironment::Kde), false),
            [Backend::Qt, Backend::Gtk]
        );
        assert_eq!(
            preference_from(None, Some(DesktopEnvironment::Lxqt), false),
            [Backend::Qt, Backend::Gtk]
        );
    }

    #[test]
    fn other_desktops_prefer_gtk() {
        assert_eq!(
            preference_from(None, Some(DesktopEnvironment::Gnome), false),
            [Backend::Gtk, Backend::Qt]
        );
        assert_eq!(
            preference_from(Some("invalid"), Some(DesktopEnvironment::Xfce), false),
            [Backend::Gtk, Backend::Qt]
        );
    }

    #[test]
    fn theme_paths_cover_toolkit_and_desktop_locations() {
        let paths = candidate_theme_paths(
            Some(Path::new("/home/test")),
            Some(Path::new("/config")),
            Some(Path::new("/data")),
            &[
                Path::new("/etc/xdg").to_owned(),
                Path::new("/opt/xdg").to_owned(),
            ],
            &[
                Path::new("/usr/share").to_owned(),
                Path::new("/opt/share").to_owned(),
            ],
        );

        for expected in [
            "/config/gtk-4.0",
            "/config/qt6ct",
            "/data/themes",
            "/home/test/.themes",
            "/etc/xdg/gtk-3.0",
            "/opt/xdg/Kvantum",
            "/usr/share/color-schemes",
            "/opt/share/themes",
        ] {
            assert!(paths.iter().any(|path| path == Path::new(expected)));
        }
    }

    #[test]
    fn portal_filter_accepts_theme_settings_only() {
        assert!(is_theme_setting(
            "org.freedesktop.appearance",
            "color-scheme"
        ));
        assert!(is_theme_setting("org.gnome.desktop.interface", "gtk-theme"));
        assert!(is_theme_setting(
            "org.gnome.desktop.interface",
            "cursor-theme"
        ));
        assert!(!is_theme_setting(
            "org.gnome.desktop.interface",
            "clock-format"
        ));
        assert!(!is_theme_setting("org.example.power", "percentage"));
    }

    #[test]
    fn change_sources_merge_for_debounced_refreshes() {
        let mut sources = ChangeSources::default();
        sources.add(ChangeSource::Portal);
        sources.add(ChangeSource::Files);
        assert_eq!(sources.label(), "desktop portal and theme files");
    }

    #[test]
    fn file_watcher_reports_theme_directory_changes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let (sender, receiver) = smol::channel::bounded(4);
        let (_watcher, count) =
            create_file_watcher(&[directory.path().to_owned()], sender).expect("file watcher");
        assert_eq!(count, 1);

        std::fs::write(directory.path().join("gtk.css"), "/* changed */")
            .expect("write watched file");
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            match receiver.try_recv() {
                Ok(ChangeSource::Files) => break,
                Ok(ChangeSource::Portal) | Err(smol::channel::TryRecvError::Empty)
                    if std::time::Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(20));
                }
                result => panic!("theme file event was not received: {result:?}"),
            }
        }
    }
}
