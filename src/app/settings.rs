use super::*;
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent, SelectItem, SelectState},
    switch::Switch,
};
use gpui_kit::{Div, FocusHandle, SharedString, Stateful, Subscription};

const SETTINGS_NAV_WIDTH: f32 = 248.;
const SETTINGS_CONTENT_WIDTH: f32 = 720.;
const SETTINGS_CONTROL_WIDTH: f32 = 220.;
const SETTINGS_INPUT_WIDTH: f32 = 320.;
const TEXT_SIZES: [u16; 13] = [
    80, 90, 100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsSection {
    Interface,
    Conversation,
    Behavior,
    Agents,
    Accessibility,
    Advanced,
}

impl SettingsSection {
    const ALL: [Self; 6] = [
        Self::Interface,
        Self::Conversation,
        Self::Behavior,
        Self::Agents,
        Self::Accessibility,
        Self::Advanced,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Interface => "Interface",
            Self::Conversation => "Conversation",
            Self::Behavior => "Behavior",
            Self::Agents => "Agents",
            Self::Advanced => "Advanced",
            Self::Accessibility => "Accessibility",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Interface => Icon::new(IconName::Palette),
            Self::Conversation => Icon::new(IconName::Bot),
            Self::Behavior => Icon::new(IconName::Settings2),
            Self::Agents => Icon::new(IconName::SquareTerminal),
            Self::Advanced => Icon::new(IconName::Cpu),
            Self::Accessibility => Icon::new(IconName::Eye),
        }
    }
}

#[derive(Clone)]
pub(super) struct SettingChoice<V> {
    value: V,
    title: SharedString,
}

impl<V: Clone + PartialEq> SelectItem for SettingChoice<V> {
    type Value = V;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &V {
        &self.value
    }
}

pub(super) type ChoiceSelect<V> = Entity<SelectState<Vec<SettingChoice<V>>>>;

pub(super) fn send_key_label(send_key: SendKey) -> &'static str {
    match send_key {
        SendKey::Enter => "Enter",
        SendKey::ShiftEnter => "Shift+Enter",
        #[cfg(target_os = "macos")]
        SendKey::CtrlEnter => "Cmd+Enter",
        #[cfg(not(target_os = "macos"))]
        SendKey::CtrlEnter => "Ctrl+Enter",
        #[cfg(target_os = "macos")]
        SendKey::AltEnter => "Option+Enter",
        #[cfg(not(target_os = "macos"))]
        SendKey::AltEnter => "Alt+Enter",
    }
}

pub(super) struct SettingsPage {
    pub(super) section: SettingsSection,
    pub(super) theme: ChoiceSelect<crate::appearance::Choice>,
    pub(super) font: ChoiceSelect<Option<SharedString>>,
    pub(super) text_size: ChoiceSelect<u16>,
    pub(super) thoughts: ChoiceSelect<bool>,
    pub(super) tool_activity: ChoiceSelect<bool>,
    pub(super) diff_layout: ChoiceSelect<DiffPresentation>,
    pub(super) send_key: ChoiceSelect<SendKey>,
    pub(super) theme_source: ChoiceSelect<ThemeSource>,
    pub(super) claude_executable: Entity<InputState>,
    pub(super) agent_executables: Vec<(KnownAgent, Entity<InputState>)>,
    pub(super) web_connect_url: Entity<InputState>,
    pub(super) new_agent_name: Entity<InputState>,
    pub(super) new_agent_command: Entity<InputState>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

fn text_size_percent(font_scale: f32) -> u16 {
    (font_scale * 100.).round() as u16
}

fn select_value<V: Clone + PartialEq + 'static>(
    select: &ChoiceSelect<V>,
    value: &V,
    window: &mut Window,
    cx: &mut App,
) {
    select.update(cx, |select, cx| {
        select.set_selected_value(value, window, cx)
    });
}

impl Workspace {
    fn choice_select<V: Clone + PartialEq + 'static>(
        choices: impl IntoIterator<Item = (V, SharedString)>,
        current: &V,
        apply: fn(&mut Self, V, &mut Context<Self>),
        subscriptions: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ChoiceSelect<V> {
        let choices: Vec<_> = choices
            .into_iter()
            .map(|(value, title)| SettingChoice { value, title })
            .collect();
        let selected = choices
            .iter()
            .position(|choice| choice.value == *current)
            .map(|index| IndexPath::default().row(index));
        let select = cx.new(|cx| SelectState::new(choices, selected, window, cx).searchable(true));
        subscriptions.push(cx.subscribe_in(
            &select,
            window,
            move |this, _, event: &SelectEvent<Vec<SettingChoice<V>>>, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    apply(this, value.clone(), cx);
                }
            },
        ));
        select
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut subscriptions = Vec::new();
        let theme = Self::choice_select(
            crate::appearance::Choice::all().map(|choice| (choice, choice.label().into())),
            &self.theme_choice,
            Self::select_theme,
            &mut subscriptions,
            window,
            cx,
        );
        let mut fonts = cx.text_system().all_font_names();
        fonts.sort_unstable();
        fonts.dedup();
        let current_font = self.font.clone().map(SharedString::from);
        let font = Self::choice_select(
            std::iter::once((None, "System default".into())).chain(
                fonts
                    .into_iter()
                    .map(|font| (Some(SharedString::from(font.clone())), font.into())),
            ),
            &current_font,
            |this, font, cx| this.set_font(font.map(String::from), cx),
            &mut subscriptions,
            window,
            cx,
        );
        let current_size = text_size_percent(self.font_scale);
        let mut sizes = TEXT_SIZES.to_vec();
        if let Err(index) = sizes.binary_search(&current_size) {
            sizes.insert(index, current_size);
        }
        let text_size = Self::choice_select(
            sizes
                .into_iter()
                .map(|size| (size, format!("{size}%").into())),
            &current_size,
            |this, size, cx| this.set_font_scale(f32::from(size) / 100., cx),
            &mut subscriptions,
            window,
            cx,
        );
        let thoughts = Self::choice_select(
            expansion_choices(),
            &self.conversation.thoughts_expanded,
            Self::set_thoughts_expanded,
            &mut subscriptions,
            window,
            cx,
        );
        let tool_activity = Self::choice_select(
            expansion_choices(),
            &self.conversation.tool_activity_expanded,
            Self::set_tool_activity_expanded,
            &mut subscriptions,
            window,
            cx,
        );
        let diff_layout = Self::choice_select(
            [
                (DiffPresentation::Unified, "Unified".into()),
                (DiffPresentation::Split, "Split".into()),
            ],
            &self.diff_layout,
            Self::set_diff_layout,
            &mut subscriptions,
            window,
            cx,
        );
        let send_key = Self::choice_select(
            SendKey::ALL.map(|send_key| (send_key, send_key_label(send_key).into())),
            &self.send_key,
            Self::set_send_key,
            &mut subscriptions,
            window,
            cx,
        );
        let theme_source = Self::choice_select(
            [
                (ThemeSource::Automatic, "Automatic".into()),
                (ThemeSource::Gtk, "GTK".into()),
                (ThemeSource::Qt, "Qt".into()),
            ],
            &self.theme_source,
            Self::set_theme_source,
            &mut subscriptions,
            window,
            cx,
        );
        let claude_executable = Self::text_setting(
            "Found on PATH",
            self.claude_executable.as_deref(),
            &mut subscriptions,
            window,
            cx,
        );
        let agent_executables = KnownAgent::ALL
            .into_iter()
            .map(|agent| {
                let input = Self::text_setting(
                    "Found on PATH",
                    self.agent_executables.get(&agent).map(String::as_str),
                    &mut subscriptions,
                    window,
                    cx,
                );
                (agent, input)
            })
            .collect();
        let web_connect_url = Self::text_setting(
            DEFAULT_WEB_CONNECT_URL,
            self.web_connect_url.as_deref(),
            &mut subscriptions,
            window,
            cx,
        );
        let new_agent_name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let new_agent_command = cx.new(|cx| InputState::new(window, cx).placeholder("Command"));
        for input in [&new_agent_name, &new_agent_command] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.add_saved_agent(window, cx);
                    }
                },
            ));
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.settings = Some(SettingsPage {
            section: SettingsSection::Interface,
            theme,
            font,
            text_size,
            thoughts,
            tool_activity,
            diff_layout,
            send_key,
            theme_source,
            claude_executable,
            agent_executables,
            web_connect_url,
            new_agent_name,
            new_agent_command,
            focus,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    fn text_setting(
        placeholder: &'static str,
        value: Option<&str>,
        subscriptions: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(value.unwrap_or_default())
        });
        subscriptions.push(cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.commit_text_settings(cx);
                }
            },
        ));
        input
    }

    pub(super) fn commit_text_settings(&mut self, cx: &mut Context<Self>) {
        let Some(page) = &self.settings else {
            return;
        };
        let text = |input: &Entity<InputState>| {
            let value = input.read(cx).value().trim().to_owned();
            (!value.is_empty()).then_some(value)
        };
        let claude_executable = text(&page.claude_executable);
        let web_connect_url = text(&page.web_connect_url);
        let agent_executables: Vec<_> = page
            .agent_executables
            .iter()
            .map(|(agent, input)| (*agent, text(input)))
            .collect();
        self.set_claude_executable(claude_executable, cx);
        self.set_web_connect_url(web_connect_url, cx);
        for (agent, path) in agent_executables {
            self.set_agent_executable(agent, path, cx);
        }
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_text_settings(cx);
        self.settings = None;
        window.focus(&self.conversation.composer.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn sync_settings_text_size(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(page) = &self.settings {
            select_value(
                &page.text_size,
                &text_size_percent(self.font_scale),
                window,
                cx,
            );
        }
    }

    pub(super) fn sync_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = &self.settings else {
            return;
        };
        select_value(&page.theme, &self.theme_choice, window, cx);
        select_value(
            &page.font,
            &self.font.clone().map(SharedString::from),
            window,
            cx,
        );
        select_value(
            &page.text_size,
            &text_size_percent(self.font_scale),
            window,
            cx,
        );
        select_value(
            &page.thoughts,
            &self.conversation.thoughts_expanded,
            window,
            cx,
        );
        select_value(
            &page.tool_activity,
            &self.conversation.tool_activity_expanded,
            window,
            cx,
        );
        select_value(&page.diff_layout, &self.diff_layout, window, cx);
        select_value(&page.send_key, &self.send_key, window, cx);
        select_value(&page.theme_source, &self.theme_source, window, cx);
        for (input, value) in [
            (&page.claude_executable, self.claude_executable.as_ref()),
            (&page.web_connect_url, self.web_connect_url.as_ref()),
        ]
        .into_iter()
        .chain(
            page.agent_executables
                .iter()
                .map(|(agent, input)| (input, self.agent_executables.get(agent))),
        ) {
            let value = value.cloned().unwrap_or_default();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    pub(super) fn set_thoughts_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        if expanded == self.conversation.thoughts_expanded {
            return;
        }
        self.conversation.thoughts_expanded = expanded;
        self.conversation.toggled_thought_rows.clear();
        for pane in self.inactive_panes.values_mut() {
            pane.conversation.thoughts_expanded = expanded;
            pane.conversation.toggled_thought_rows.clear();
        }
        self.persist();
        cx.notify();
    }

    pub(super) fn set_tool_activity_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        if expanded == self.conversation.tool_activity_expanded {
            return;
        }
        self.conversation.tool_activity_expanded = expanded;
        self.conversation.toggled_tool_groups.clear();
        for pane in self.inactive_panes.values_mut() {
            pane.conversation.tool_activity_expanded = expanded;
            pane.conversation.toggled_tool_groups.clear();
        }
        self.persist();
        cx.notify();
    }

    pub(super) fn set_diff_layout(&mut self, layout: DiffPresentation, cx: &mut Context<Self>) {
        if layout == self.diff_layout {
            return;
        }
        self.diff_layout = layout;
        self.set_diff_presentation(layout, cx);
        self.persist();
        cx.notify();
    }

    pub(super) fn set_font(&mut self, font: Option<String>, cx: &mut Context<Self>) {
        if font == self.font {
            return;
        }
        crate::appearance::set_font(font.clone().map(SharedString::from), cx);
        self.font = font;
        self.select_theme(self.theme_choice, cx);
    }

    pub(super) fn set_send_key(&mut self, send_key: SendKey, cx: &mut Context<Self>) {
        if send_key == self.send_key {
            return;
        }
        self.send_key = send_key;
        let submit_on_enter = send_key == SendKey::Enter;
        for composer in std::iter::once(&self.conversation.composer)
            .chain(self.session_composers.values())
            .chain(
                self.inactive_panes
                    .values()
                    .map(|pane| &pane.conversation.composer),
            )
        {
            composer.update(cx, |input, cx| {
                input.set_submit_on_enter(submit_on_enter, cx)
            });
        }
        self.persist();
        cx.notify();
    }

    pub(super) fn set_nested_sidebar(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.nested_sidebar == enabled {
            return;
        }
        self.nested_sidebar = enabled;
        self.sidebar_selection = 0;
        self.persist();
        cx.notify();
    }

    pub(super) fn set_reduced_motion(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled == self.reduced_motion {
            return;
        }
        self.reduced_motion = enabled;
        cx.set_reduce_motion(enabled);
        self.persist();
        cx.notify();
    }

    pub(super) fn set_claude_executable(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        if path != self.claude_executable {
            self.claude_executable = path;
            self.persist();
            cx.notify();
        }
    }

    pub(super) fn set_agent_executable(
        &mut self,
        agent: KnownAgent,
        path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.agent_executables.get(&agent) == path.as_ref() {
            return;
        }
        match path {
            Some(path) => self.agent_executables.insert(agent, path),
            None => self.agent_executables.remove(&agent),
        };
        self.picker.available_agents = agent_choices(&self.saved_agents, &self.agent_executables);
        self.persist();
        cx.notify();
    }

    pub(super) fn set_web_connect_url(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        if url != self.web_connect_url {
            self.web_connect_url = url;
            self.persist();
            cx.notify();
        }
    }

    pub(super) fn set_theme_source(&mut self, source: ThemeSource, cx: &mut Context<Self>) {
        if source != self.theme_source {
            self.theme_source = source;
            crate::theming::set_source(source, cx);
            self.persist();
            cx.notify();
        }
    }

    pub(super) fn add_saved_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = &self.settings else {
            return;
        };
        let (name_input, command_input) =
            (page.new_agent_name.clone(), page.new_agent_command.clone());
        let name = name_input.read(cx).value().trim().to_owned();
        let Ok(command) = shell_words::split(command_input.read(cx).value().as_ref()) else {
            return;
        };
        if name.is_empty() || command.is_empty() {
            return;
        }
        self.saved_agents.push(SavedAgent { name, command });
        self.picker.available_agents = agent_choices(&self.saved_agents, &self.agent_executables);
        for input in [name_input, command_input] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.persist();
        cx.notify();
    }

    pub(super) fn remove_saved_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.saved_agents.len() {
            self.saved_agents.remove(index);
            self.picker.available_agents =
                agent_choices(&self.saved_agents, &self.agent_executables);
            self.persist();
            cx.notify();
        }
    }

    pub(super) fn reset_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_nested_sidebar(Config::default().nested_sidebar, cx);
        self.set_font(None, cx);
        self.select_theme(crate::appearance::Choice::default(), cx);
        self.set_font_scale(1.0, cx);
        self.set_thoughts_expanded(false, cx);
        self.set_tool_activity_expanded(true, cx);
        self.set_diff_layout(DiffPresentation::default(), cx);
        self.set_send_key(SendKey::default(), cx);
        self.set_notifications(true, cx);
        self.set_reduced_motion(false, cx);
        self.set_claude_executable(None, cx);
        for agent in KnownAgent::ALL {
            self.set_agent_executable(agent, None, cx);
        }
        self.set_web_connect_url(None, cx);
        self.set_theme_source(ThemeSource::default(), cx);
        self.sync_settings(window, cx);
    }

    pub(super) fn render_settings(&self, page: &SettingsPage, cx: &mut Context<Self>) -> Div {
        let palette = theme::palette(cx);
        let section = page.section;
        let content = match section {
            SettingsSection::Interface => settings_group(
                [
                    setting_row("Theme", settings_select(&page.theme), palette),
                    setting_row("Font", settings_select(&page.font), palette),
                    setting_row("Text size", settings_select(&page.text_size), palette),
                    setting_row(
                        "Group sessions by project",
                        Switch::new("settings-nested-sidebar")
                            .checked(self.nested_sidebar)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.set_nested_sidebar(*checked, cx);
                            })),
                        palette,
                    ),
                ],
                palette,
            ),
            SettingsSection::Conversation => settings_group(
                [
                    setting_row("Thoughts", settings_select(&page.thoughts), palette),
                    setting_row(
                        "Tool activity",
                        settings_select(&page.tool_activity),
                        palette,
                    ),
                    setting_row("Diff layout", settings_select(&page.diff_layout), palette),
                ],
                palette,
            ),
            SettingsSection::Behavior => settings_group(
                [
                    setting_row(
                        "Send message with",
                        settings_select(&page.send_key),
                        palette,
                    ),
                    setting_row(
                        "Desktop notifications",
                        Switch::new("settings-notifications")
                            .checked(self.notifications)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.set_notifications(*checked, cx)
                            })),
                        palette,
                    ),
                ],
                palette,
            ),
            SettingsSection::Agents => div()
                .flex()
                .flex_col()
                .gap_4()
                .child(settings_group(
                    page.agent_executables
                        .iter()
                        .map(|(agent, input)| {
                            setting_row(
                                agent_label(agent_icon(*agent), agent_executable_label(*agent)),
                                settings_input(input),
                                palette,
                            )
                        })
                        .chain(std::iter::once(setting_row(
                            agent_label(agent_icon(KnownAgent::Claude), "Claude Code executable"),
                            settings_input(&page.claude_executable),
                            palette,
                        ))),
                    palette,
                ))
                .child(settings_group_label("Saved agents", palette))
                .child(settings_group(
                    self.saved_agents
                        .iter()
                        .enumerate()
                        .map(|(index, agent)| {
                            saved_agent_row(agent, palette).child(
                                Button::new(("settings-remove-agent", index))
                                    .ghost()
                                    .compact()
                                    .icon(IconName::CircleX)
                                    .tooltip("Remove")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_saved_agent(index, cx)
                                    })),
                            )
                        })
                        .chain(std::iter::once(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_4()
                                .py_3()
                                .child(div().w(px(160.)).child(Input::new(&page.new_agent_name)))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .child(Input::new(&page.new_agent_command)),
                                )
                                .child(
                                    Button::new("settings-add-agent")
                                        .label("Add agent")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.add_saved_agent(window, cx)
                                        })),
                                ),
                        )),
                    palette,
                )),
            SettingsSection::Advanced => settings_group(
                std::iter::once(setting_row(
                    "Web Connect address",
                    settings_input(&page.web_connect_url),
                    palette,
                ))
                .chain(cfg!(target_os = "linux").then(|| {
                    setting_row(
                        "System theme source",
                        settings_select(&page.theme_source),
                        palette,
                    )
                }))
                .chain(std::iter::once(setting_row(
                    "Restore default settings",
                    Button::new("settings-reset")
                        .icon(IconName::Undo2)
                        .label("Factory reset")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.reset_settings(window, cx)),
                        ),
                    palette,
                ))),
                palette,
            ),
            SettingsSection::Accessibility => settings_group(
                [setting_row(
                    "Reduce motion",
                    Switch::new("settings-reduce-motion")
                        .checked(self.reduced_motion)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.set_reduced_motion(*checked, cx)
                        })),
                    palette,
                )],
                palette,
            ),
        };
        div()
            .track_focus(&page.focus)
            .size_full()
            .flex()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && this
                        .settings
                        .as_ref()
                        .is_some_and(|page| page.focus.is_focused(window))
                {
                    this.close_settings(window, cx);
                }
            }))
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .w(px(SETTINGS_NAV_WIDTH))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .bg(palette.color(SIDEBAR))
                    .border_r_1()
                    .border_color(palette.color(BORDER))
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(palette.color(MUTED))
                            .child("Settings"),
                    )
                    .children(SettingsSection::ALL.map(|candidate| {
                        settings_nav_row(candidate, candidate == section, palette).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Some(page) = &mut this.settings {
                                    page.section = candidate;
                                }
                                cx.notify();
                            }),
                        )
                    }))
                    .child(div().flex_1())
                    .child(
                        Button::new("close-settings")
                            .ghost()
                            .w_full()
                            .icon(IconName::ArrowLeft)
                            .tooltip("Back")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_settings(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("settings-detail")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_6()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(palette.color(TEXT))
                            .child(section.label()),
                    )
                    .child(content),
            )
    }
}

fn agent_label(icon: Icon, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(icon.size(px(16.)))
        .child(label)
}

fn agent_executable_label(agent: KnownAgent) -> &'static str {
    match agent {
        KnownAgent::Codex => "Codex ACP adapter",
        KnownAgent::Claude => "Claude ACP adapter",
        KnownAgent::GeminiCli => "Gemini CLI",
        KnownAgent::OpenCode => "OpenCode",
    }
}

fn settings_input(input: &Entity<InputState>) -> Div {
    div().w(px(SETTINGS_INPUT_WIDTH)).child(Input::new(input))
}

fn settings_group_label(label: &'static str, palette: Palette) -> Div {
    div()
        .text_sm()
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .text_color(palette.color(MUTED))
        .child(label)
}

fn saved_agent_row(agent: &SavedAgent, palette: Palette) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .child(
            div()
                .text_color(palette.color(TEXT))
                .child(agent.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .font_family("monospace")
                .text_sm()
                .text_color(palette.color(MUTED))
                .child(shell_words::join(&agent.command)),
        )
}

fn expansion_choices() -> [(bool, SharedString); 2] {
    [(false, "Collapsed".into()), (true, "Expanded".into())]
}

fn settings_select<V: Clone + PartialEq + 'static>(select: &ChoiceSelect<V>) -> Div {
    div()
        .w(px(SETTINGS_CONTROL_WIDTH))
        .child(Select::new(select).w_full())
}

fn settings_nav_row(section: SettingsSection, selected: bool, palette: Palette) -> Stateful<Div> {
    div()
        .id(("settings-section", section as usize))
        .w_full()
        .h(px(30.))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .text_sm()
        .text_color(palette.color(if selected { TEXT } else { MUTED }))
        .when(selected, |row| row.bg(palette.color(SELECTED)))
        .when(!selected, |row| {
            row.hover(|style| style.bg(palette.color(HOVER)))
        })
        .child(section.icon().size(px(16.)))
        .child(section.label())
}

fn settings_group(rows: impl IntoIterator<Item = Div>, palette: Palette) -> Div {
    div()
        .w_full()
        .max_w(px(SETTINGS_CONTENT_WIDTH))
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(palette.color(BORDER))
        .bg(palette.color(SURFACE))
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            row.when(index > 0, |row| {
                row.border_t_1().border_color(palette.color(BORDER))
            })
        }))
}

fn setting_row(label: impl IntoElement, control: impl IntoElement, palette: Palette) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .px_4()
        .py_3()
        .child(div().text_color(palette.color(TEXT)).child(label))
        .child(control)
}
