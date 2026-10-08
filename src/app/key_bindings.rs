use super::*;
use gpui_kit::{KeybindingKeystroke, Keystroke, KeystrokeEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shortcut {
    QuickOpen,
    ToggleTerminal,
    NewSession,
    OpenSettings,
    ToggleSidebar,
    ToggleDiff,
    SearchSessions,
    NextSession,
    PreviousSession,
    Minimize,
    Hide,
    HideOthers,
    Session1,
    Session2,
    Session3,
    Session4,
    Session5,
    Session6,
    Session7,
    Session8,
    Session9,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Quit,
    Send,
    Newline,
    Previous,
    Next,
    Confirm,
    Complete,
    Dismiss,
}

impl Shortcut {
    pub(super) const ALL: [Self; 32] = [
        Self::QuickOpen,
        Self::ToggleTerminal,
        Self::NewSession,
        Self::OpenSettings,
        Self::ToggleSidebar,
        Self::ToggleDiff,
        Self::SearchSessions,
        Self::NextSession,
        Self::PreviousSession,
        Self::Minimize,
        Self::Hide,
        Self::HideOthers,
        Self::Session1,
        Self::Session2,
        Self::Session3,
        Self::Session4,
        Self::Session5,
        Self::Session6,
        Self::Session7,
        Self::Session8,
        Self::Session9,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ZoomReset,
        Self::Quit,
        Self::Send,
        Self::Newline,
        Self::Previous,
        Self::Next,
        Self::Confirm,
        Self::Complete,
        Self::Dismiss,
    ];

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::QuickOpen => "quick_open",
            Self::ToggleTerminal => "toggle_terminal",
            Self::NewSession => "new_session",
            Self::OpenSettings => "open_settings",
            Self::ToggleSidebar => "toggle_sidebar",
            Self::ToggleDiff => "toggle_diff",
            Self::SearchSessions => "search_sessions",
            Self::NextSession => "next_session",
            Self::PreviousSession => "previous_session",
            Self::Minimize => "minimize",
            Self::Hide => "hide",
            Self::HideOthers => "hide_others",
            Self::Session1 => "session_1",
            Self::Session2 => "session_2",
            Self::Session3 => "session_3",
            Self::Session4 => "session_4",
            Self::Session5 => "session_5",
            Self::Session6 => "session_6",
            Self::Session7 => "session_7",
            Self::Session8 => "session_8",
            Self::Session9 => "session_9",

            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::ZoomReset => "zoom_reset",
            Self::Quit => "quit",
            Self::Send => "send",
            Self::Newline => "newline",
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Confirm => "confirm",
            Self::Complete => "complete",
            Self::Dismiss => "dismiss",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::QuickOpen => "New session in folder",
            Self::ToggleTerminal => "Show or hide the session terminal",
            Self::NewSession => "New session",
            Self::OpenSettings => "Open settings",
            Self::ToggleSidebar => "Show or hide sidebar",
            Self::ToggleDiff => "Show or hide diff",
            Self::SearchSessions => "Search sessions",
            Self::NextSession => "Next session",
            Self::PreviousSession => "Previous session",
            Self::Minimize => "Minimize window",
            Self::Hide => "Hide Agentaps",
            Self::HideOthers => "Hide other apps",
            Self::Session1 => "Go to session 1",
            Self::Session2 => "Go to session 2",
            Self::Session3 => "Go to session 3",
            Self::Session4 => "Go to session 4",
            Self::Session5 => "Go to session 5",
            Self::Session6 => "Go to session 6",
            Self::Session7 => "Go to session 7",
            Self::Session8 => "Go to session 8",
            Self::Session9 => "Go to session 9",

            Self::ZoomIn => "Increase text size",
            Self::ZoomOut => "Decrease text size",
            Self::ZoomReset => "Reset text size",
            Self::Quit => "Quit Agentaps",
            Self::Send => "Send message",
            Self::Newline => "Insert a newline",
            Self::Previous => "Previous result, suggestion, or prompt",
            Self::Next => "Next result, suggestion, or prompt",
            Self::Confirm => "Choose a picker or session search result",
            Self::Complete => "Complete a command or file suggestion",
            Self::Dismiss => "Stop the active turn, dismiss, or go back",
        }
    }

    pub(super) fn available(self) -> bool {
        (!matches!(
            self,
            Self::Quit | Self::Minimize | Self::Hide | Self::HideOthers
        ) || cfg!(target_os = "macos"))
            && (self != Self::ToggleTerminal
                || cfg!(all(
                    feature = "ghostty-terminal",
                    any(target_os = "linux", target_os = "macos")
                )))
    }

    fn global(self) -> bool {
        !matches!(
            self,
            Self::Send
                | Self::Newline
                | Self::Previous
                | Self::Next
                | Self::Confirm
                | Self::Complete
                | Self::Dismiss
        )
    }

    fn action(self) -> Option<Box<dyn gpui_kit::Action>> {
        match self {
            Self::QuickOpen => Some(Box::new(QuickOpen)),
            Self::ZoomIn => Some(Box::new(ZoomIn)),
            Self::ZoomOut => Some(Box::new(ZoomOut)),
            Self::ZoomReset => Some(Box::new(ZoomReset)),
            Self::Quit => Some(Box::new(Quit)),
            Self::NewSession => Some(Box::new(NewSession)),
            Self::OpenSettings => Some(Box::new(OpenSettings)),
            Self::ToggleSidebar => Some(Box::new(ToggleSidebar)),
            Self::ToggleDiff => Some(Box::new(ToggleDiff)),
            Self::SearchSessions => Some(Box::new(SearchSessions)),
            Self::NextSession => Some(Box::new(NextSession)),
            Self::PreviousSession => Some(Box::new(PreviousSession)),
            Self::Minimize => Some(Box::new(Minimize)),
            Self::Hide => Some(Box::new(Hide)),
            Self::HideOthers => Some(Box::new(HideOthers)),
            Self::Session1 => Some(Box::new(ActivateSession(0))),
            Self::Session2 => Some(Box::new(ActivateSession(1))),
            Self::Session3 => Some(Box::new(ActivateSession(2))),
            Self::Session4 => Some(Box::new(ActivateSession(3))),
            Self::Session5 => Some(Box::new(ActivateSession(4))),
            Self::Session6 => Some(Box::new(ActivateSession(5))),
            Self::Session7 => Some(Box::new(ActivateSession(6))),
            Self::Session8 => Some(Box::new(ActivateSession(7))),
            Self::Session9 => Some(Box::new(ActivateSession(8))),
            _ => None,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.global()
            || other.global()
            || (!matches!(self, Self::Send | Self::Newline | Self::Complete)
                && !matches!(other, Self::Send | Self::Newline | Self::Complete))
            || (self != Self::Confirm && other != Self::Confirm)
    }

    fn defaults(self, send_key: SendKey) -> Vec<Keystroke> {
        if let Some(action) = self.action() {
            return super::key_bindings()
                .into_iter()
                .filter(|binding| binding.action().partial_eq(action.as_ref()))
                .flat_map(|binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(|key| key.inner().clone())
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        let keys = match self {
            Self::ToggleTerminal => vec!["ctrl-t"],
            Self::Send => vec![match send_key {
                SendKey::Enter => "enter",
                SendKey::ShiftEnter => "shift-enter",
                SendKey::CtrlEnter => "secondary-enter",
                SendKey::AltEnter => "alt-enter",
            }],
            Self::Newline if send_key == SendKey::Enter => vec!["shift-enter", "ctrl-enter"],
            Self::Newline => vec!["enter"],
            Self::Previous => vec!["up"],
            Self::Next => vec!["down"],
            Self::Confirm => vec!["enter"],
            Self::Complete => vec!["tab"],
            Self::Dismiss => vec!["escape"],
            _ => unreachable!("workspace shortcuts have actions"),
        };
        keys.into_iter()
            .map(|key| Keystroke::parse(key).unwrap())
            .collect()
    }

    fn default_keys(
        self,
        bindings: &BTreeMap<String, String>,
        send_key: SendKey,
    ) -> Vec<Keystroke> {
        if self == Self::Newline
            && let Some(send) = bindings
                .get(Self::Send.id())
                .and_then(|key| Keystroke::parse(key).ok())
        {
            return self.defaults(
                if send.key == "enter" && send.modifiers == Default::default() {
                    SendKey::Enter
                } else {
                    SendKey::CtrlEnter
                },
            );
        }
        self.defaults(send_key)
    }

    fn keys(self, bindings: &BTreeMap<String, String>, send_key: SendKey) -> Vec<Keystroke> {
        if let Some(key) = bindings.get(self.id()) {
            Keystroke::parse(key).into_iter().collect()
        } else {
            self.default_keys(bindings, send_key)
        }
    }
}

fn matches(key: &Keystroke, binding: &Keystroke) -> bool {
    key.should_match(&KeybindingKeystroke::from_keystroke(binding.clone()))
}

fn conflict(
    shortcut: Shortcut,
    key: &Keystroke,
    bindings: &BTreeMap<String, String>,
    send_key: SendKey,
) -> Option<Shortcut> {
    Shortcut::ALL.into_iter().find(|other| {
        *other != shortcut
            && other.available()
            && shortcut.overlaps(*other)
            && other
                .keys(bindings, send_key)
                .iter()
                .any(|binding| matches(key, binding))
    })
}

pub(super) fn validated(
    saved: BTreeMap<String, String>,
    send_key: SendKey,
) -> BTreeMap<String, String> {
    // Preserve valid pairs that have exchanged defaults; discard unknown or malformed entries.
    let mut bindings: BTreeMap<_, _> = saved
        .into_iter()
        .filter(|(id, key)| {
            Shortcut::ALL
                .iter()
                .any(|shortcut| shortcut.id() == id && shortcut.available())
                && Keystroke::parse(key).is_ok()
        })
        .collect();
    loop {
        let conflicting = Shortcut::ALL.into_iter().find(|shortcut| {
            bindings
                .get(shortcut.id())
                .and_then(|key| Keystroke::parse(key).ok())
                .is_some_and(|key| conflict(*shortcut, &key, &bindings, send_key).is_some())
        });
        let Some(shortcut) = conflicting else {
            break;
        };
        bindings.remove(shortcut.id());
    }
    bindings
}

impl Workspace {
    pub(super) fn shortcut_customized(&self, shortcut: Shortcut) -> bool {
        self.key_bindings.contains_key(shortcut.id())
    }

    pub(super) fn shortcut_label(&self, shortcut: Shortcut) -> String {
        shortcut
            .keys(&self.key_bindings, self.send_key)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub(super) fn composer_submits_on_enter(&self) -> bool {
        self.send_key == SendKey::Enter && !self.shortcut_customized(Shortcut::Send)
    }

    pub(super) fn sync_composer_send_key(&mut self, cx: &mut Context<Self>) {
        let submit = self.composer_submits_on_enter();
        for composer in std::iter::once(&self.conversation.composer)
            .chain(self.session_composers.values())
            .chain(
                self.inactive_panes
                    .values()
                    .map(|pane| &pane.conversation.composer),
            )
        {
            composer.update(cx, |input, cx| input.set_submit_on_enter(submit, cx));
        }
    }

    pub(super) fn navigation_key<'a>(&self, key: &'a str) -> &'a str {
        let shortcut = match key {
            "up" => Shortcut::Previous,
            "down" => Shortcut::Next,
            "escape" => Shortcut::Dismiss,
            _ => return key,
        };
        if self.shortcut_customized(shortcut) {
            ""
        } else {
            key
        }
    }

    pub(super) fn intercept_shortcut(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(shortcut) = self.settings.as_ref().and_then(|page| page.recording) {
            cx.stop_propagation();
            let key = &event.keystroke;
            if matches!(
                key.key.as_str(),
                "shift" | "control" | "ctrl" | "alt" | "cmd" | "super" | "fn"
            ) {
                return;
            }
            if key.key == "escape" && key.modifiers == Default::default() {
                let page = self.settings.as_mut().unwrap();
                page.recording = None;
                page.binding_error = None;
            } else {
                let mut proposed = self.key_bindings.clone();
                proposed.insert(shortcut.id().into(), key.unparse());
                if let Some(other) = conflict(shortcut, key, &proposed, self.send_key) {
                    self.settings.as_mut().unwrap().binding_error = Some(format!(
                        "Already used for {}. Press another key combination.",
                        other.label()
                    ));
                } else {
                    self.key_bindings = proposed;
                    let page = self.settings.as_mut().unwrap();
                    page.recording = None;
                    page.binding_error = None;
                    self.sync_composer_send_key(cx);
                    self.persist();
                }
            }
            cx.notify();
            return;
        }
        self.activate_keyboard_pane(window, cx);
        let composer = self.settings.is_none() && self.composer_focused(window, cx);
        let sidebar = self.settings.is_none()
            && self
                .sidebar_search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
        let picker = self.settings.is_none()
            && matches!(self.view, WorkspaceView::NewSession { .. })
            && (self
                .picker
                .input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
                || self
                    .picker
                    .clone
                    .name
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window));
        let page_focused = self
            .settings
            .as_ref()
            .is_some_and(|page| page.focus.is_focused(window));
        let mut retired_default = false;
        for shortcut in Shortcut::ALL
            .into_iter()
            .filter(|shortcut| shortcut.available())
        {
            #[cfg(all(
                feature = "ghostty-terminal",
                any(target_os = "linux", target_os = "macos")
            ))]
            if !shortcut.global() && self.terminal_focused(window, cx) {
                continue;
            }
            if !self.shortcut_customized(shortcut) && shortcut != Shortcut::ToggleTerminal {
                continue;
            }
            let applicable = shortcut.global()
                || match shortcut {
                    Shortcut::Send | Shortcut::Newline | Shortcut::Complete => composer,
                    Shortcut::Previous | Shortcut::Next => composer || sidebar || picker,
                    Shortcut::Confirm => sidebar || picker,
                    Shortcut::Dismiss => self.settings.is_none() || page_focused,
                    _ => false,
                };
            if !applicable {
                continue;
            }
            if shortcut
                .keys(&self.key_bindings, self.send_key)
                .iter()
                .any(|binding| matches(&event.keystroke, binding))
            {
                cx.stop_propagation();
                match shortcut {
                    Shortcut::ToggleTerminal => {
                        #[cfg(all(
                            feature = "ghostty-terminal",
                            any(target_os = "linux", target_os = "macos")
                        ))]
                        self.toggle_active_terminal(window, cx);
                    }
                    Shortcut::Send => {
                        if self.file_results(cx).is_empty() && self.slash_results(cx).is_empty() {
                            self.send_prompt(window, cx);
                        } else {
                            self.handle_slash_action(SlashAction::Complete, window, cx);
                        }
                    }
                    Shortcut::Newline => self
                        .conversation
                        .composer
                        .update(cx, |input, cx| input.replace("\n", window, cx)),
                    Shortcut::Complete => {
                        self.handle_slash_action(SlashAction::Complete, window, cx)
                    }
                    Shortcut::Confirm if sidebar => self.confirm_sidebar_session(window, cx),
                    Shortcut::Confirm => self.confirm_picker(window, cx),
                    Shortcut::Dismiss if page_focused => self.close_settings(window, cx),
                    Shortcut::Dismiss if !sidebar && !picker => self.handle_escape(window, cx),
                    Shortcut::Previous | Shortcut::Next | Shortcut::Dismiss if composer => {
                        match shortcut {
                            Shortcut::Previous => {
                                self.handle_slash_action(SlashAction::Up, window, cx)
                            }
                            Shortcut::Next => {
                                self.handle_slash_action(SlashAction::Down, window, cx)
                            }
                            _ => self.handle_escape(window, cx),
                        }
                    }
                    Shortcut::Previous | Shortcut::Next | Shortcut::Dismiss => {
                        let key = match shortcut {
                            Shortcut::Previous => "up",
                            Shortcut::Next => "down",
                            _ => "escape",
                        };
                        // Run the existing picker/search behavior with its logical key.
                        self.workspace_key_down_with_key(
                            &KeyDownEvent {
                                keystroke: Keystroke::parse(key).unwrap(),
                                is_held: false,
                                prefer_character_input: false,
                            },
                            Some(key),
                            window,
                            cx,
                        );
                    }
                    _ => window.dispatch_action(shortcut.action().unwrap(), cx),
                }
                return;
            }
            // Retire global defaults and newline aliases after rebinding. Input navigation
            // defaults still move the caret, but no longer trigger workspace actions.
            if (shortcut.global() || shortcut == Shortcut::Newline)
                && shortcut
                    .default_keys(&self.key_bindings, self.send_key)
                    .iter()
                    .any(|binding| matches(&event.keystroke, binding))
            {
                retired_default = true;
            }
        }
        if retired_default {
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_reject_overlapping_shortcuts_but_allow_separate_contexts() {
        let bindings = BTreeMap::new();
        if Shortcut::ToggleTerminal.available() {
            let toggle = Shortcut::ToggleTerminal.keys(&bindings, SendKey::Enter);
            assert_eq!(toggle.len(), 1);
            assert_eq!(toggle[0], Keystroke::parse("ctrl-t").unwrap());
            assert_eq!(
                conflict(Shortcut::QuickOpen, &toggle[0], &bindings, SendKey::Enter),
                Some(Shortcut::ToggleTerminal)
            );
        }
        let enter = Keystroke::parse("enter").unwrap();
        assert_eq!(
            conflict(Shortcut::Send, &enter, &bindings, SendKey::Enter),
            None
        );
        assert_eq!(
            conflict(Shortcut::ZoomIn, &enter, &bindings, SendKey::Enter),
            Some(Shortcut::Send)
        );
        let proposed = BTreeMap::from([("send".into(), "secondary-enter".into())]);
        let secondary = Keystroke::parse("secondary-enter").unwrap();
        assert_eq!(
            conflict(Shortcut::Send, &secondary, &proposed, SendKey::Enter),
            None
        );
        assert_eq!(
            Shortcut::Newline.keys(&proposed, SendKey::Enter)[0].key,
            "enter"
        );
    }

    #[test]
    fn restores_valid_bindings_and_discards_invalid_or_conflicting_entries() {
        let saved = BTreeMap::from([
            ("send".into(), "f7".into()),
            ("newline".into(), "f8".into()),
            ("unknown".into(), "f9".into()),
            ("complete".into(), "not-a-modifier-enter".into()),
            ("zoom_in".into(), "secondary-shift-n".into()),
        ]);
        let valid = validated(saved, SendKey::Enter);
        assert_eq!(
            valid,
            BTreeMap::from([
                ("send".into(), "f7".into()),
                ("newline".into(), "f8".into())
            ])
        );
        let config = Config {
            key_bindings: valid.clone(),
            ..Config::default()
        };
        let restored: Config =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(restored.key_bindings, valid);
        assert!(
            serde_json::from_str::<Config>("{}")
                .unwrap()
                .key_bindings
                .is_empty()
        );
    }
}
