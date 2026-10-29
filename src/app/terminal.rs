//! Session-owned Rio terminals. Hiding a view never drops its shell.
use super::*;
use gpui_kit::Div;
gpui_rio::bind_gpui!(gpui_kit);
use gpui_rio::{TerminalColor, TerminalOptions, TerminalStyle, TerminalTheme};

pub(super) struct SessionTerminal {
    entity: Entity<Terminal>,
    focus: gpui_kit::FocusHandle,
    style: TerminalStyle,
    height: Option<f32>,
    pub(super) open: bool,
}

#[derive(Clone)]
pub(super) struct TerminalResize {
    agent_id: u64,
    pane_id: u64,
}

impl Render for TerminalResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn terminal_height(preferred: Option<f32>, pane_height: f32, scale: f32) -> f32 {
    // Leave space for the composer and some conversation, even in a short split pane.
    let maximum = (pane_height * 0.65).min((pane_height - 160. * scale).max(0.));
    let minimum = 80_f32.min(maximum);
    preferred
        .unwrap_or_else(|| (pane_height * 0.35).min(240. * scale))
        .clamp(minimum, maximum)
}

fn terminal_theme(cx: &App) -> TerminalTheme {
    let palette = theme::palette(cx);
    let foreground = palette.color(TEXT);
    let blue = palette.color(ACCENT);
    let green = palette.color(STATUS_DONE);
    let colors = [
        palette.color(BG),
        palette.color(STATUS_ERROR),
        green,
        palette.color(STATUS_WORKING),
        blue,
        palette.color(STATUS_QUESTION),
        green.blend(blue.opacity(0.5)),
        foreground,
    ];
    let color = |value: gpui_kit::Hsla| {
        let rgb = value.to_rgb();
        TerminalColor {
            r: (rgb.r * 255.).round() as u8,
            g: (rgb.g * 255.).round() as u8,
            b: (rgb.b * 255.).round() as u8,
        }
    };
    let ansi = std::array::from_fn(|index| match index {
        0..=7 => color(colors[index]),
        8 => color(palette.color(MUTED)),
        15 => color(foreground),
        _ => color(colors[index - 8].blend(foreground.opacity(0.15))),
    });
    TerminalTheme::new(color(palette.color(BG)), color(foreground), ansi)
}

fn terminal_style(cx: &App) -> TerminalStyle {
    let theme = gpui_kit::component::theme::Theme::global(cx);
    TerminalStyle {
        theme: terminal_theme(cx),
        font_family: theme.mono_font_family.to_string(),
        font_size: f32::from(theme.mono_font_size),
    }
}

impl Workspace {
    pub(super) fn focus_terminal(
        &mut self,
        agent_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(terminal) = self
            .terminals
            .get(&agent_id)
            .filter(|terminal| terminal.open)
        else {
            return false;
        };
        terminal
            .entity
            .update(cx, |terminal, cx| terminal.focus(window, cx));
        true
    }

    pub(super) fn start_project_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let WorkspaceView::NewSession {
            step: PickerStep::Agents { project_index },
            ..
        } = self.view
        else {
            return;
        };
        if !self.terminal_picker_available(cx) {
            return;
        }
        let (id, agent_index) = self.create_session_for_project(
            project_index,
            Vec::new(),
            Some("Terminal".into()),
            true,
            cx,
        );
        self.set_view(
            WorkspaceView::Conversation(SessionLocation {
                project_index,
                agent_index,
            }),
            window,
            cx,
        );
        self.start_terminal(id, window, cx);
    }

    pub(super) fn resize_terminal(
        &mut self,
        event: &DragMoveEvent<TerminalResize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx);
        // Clear a stale drag when the divider no longer has the left button.
        if event.event.pressed_button != Some(MouseButton::Left) {
            cx.stop_active_drag(window);
            return;
        }
        let Some(bounds) = self
            .pane_bounds
            .get(&drag.pane_id)
            .map(|bounds| bounds.get())
        else {
            return;
        };
        let Some(terminal) = self
            .terminals
            .get_mut(&drag.agent_id)
            .filter(|terminal| terminal.open)
        else {
            return;
        };
        terminal.height = Some(terminal_height(
            Some(f32::from(bounds.bottom() - event.event.position.y)),
            f32::from(bounds.size.height),
            self.font_scale,
        ));
        terminal
            .entity
            .update(cx, |terminal, cx| terminal.focus(window, cx));
        cx.notify();
    }

    pub(super) fn toggle_active_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some()
            || self.mobile_access.pairing_visible
            || self.mobile_access.provider_prompt.is_some()
        {
            return;
        }
        let Some(session) = self.view.displayed_session() else {
            return;
        };
        let project = &self.projects[session.project_index];
        if project.agents[session.agent_index].config.archived {
            return;
        }
        if project.ssh_host.is_some() {
            return;
        }
        let agent_id = project.agents[session.agent_index].config.id;
        self.toggle_terminal(agent_id, window, cx);
    }

    pub(super) fn close_exited_terminals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let exited: Vec<_> = self
            .terminals
            .iter()
            .filter_map(|(&id, terminal)| {
                (terminal.open && !terminal.entity.read(cx).is_alive())
                    .then_some((id, terminal.focus.contains_focused(window, cx)))
            })
            .collect();
        for (agent_id, focused) in exited {
            let terminal = self.terminals.get_mut(&agent_id).unwrap();
            terminal.open = false;
            terminal
                .entity
                .update(cx, |terminal, _| terminal.set_visible(false));
            if focused {
                let pane = self
                    .pane_layout
                    .root
                    .panes()
                    .into_iter()
                    .find_map(|(id, _)| {
                        let session = self.pane_session(id)?;
                        (self.projects[session.project_index].agents[session.agent_index]
                            .config
                            .id
                            == agent_id)
                            .then_some(id)
                    });
                if let Some(pane) = pane
                    && self.activate_pane(pane, cx)
                {
                    self.focus_session_input(window, cx);
                }
            }
            cx.notify();
        }
    }

    pub(super) fn toggle_terminal(
        &mut self,
        agent_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .terminals
            .get(&agent_id)
            .is_some_and(|terminal| !terminal.entity.read(cx).is_alive())
        {
            self.start_terminal(agent_id, window, cx);
            return;
        }
        if let Some(terminal) = self.terminals.get_mut(&agent_id) {
            terminal.open = !terminal.open;
            let open = terminal.open;
            terminal.entity.update(cx, |terminal, cx| {
                terminal.set_visible(open);
                if open {
                    terminal.focus(window, cx);
                }
            });
            if !open {
                self.focus_session_input(window, cx);
            }
            cx.notify();
            return;
        }
        self.start_terminal(agent_id, window, cx);
    }

    fn start_terminal(&mut self, agent_id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session_location(agent_id) else {
            return;
        };
        let project = &self.projects[session.project_index];
        if project.agents[session.agent_index].config.archived {
            return;
        }
        if project.ssh_host.is_some() {
            self.notice = Some(Notice::Info("The terminal supports local projects.".into()));
            cx.notify();
            return;
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let style = terminal_style(cx);
        let height = self
            .terminals
            .get(&agent_id)
            .and_then(|terminal| terminal.height);
        match Terminal::spawn(
            TerminalOptions::new(shell, &project.path, style.clone()),
            cx,
        ) {
            Ok(entity) => {
                entity.update(cx, |terminal, cx| terminal.focus(window, cx));
                self.terminals.insert(
                    agent_id,
                    SessionTerminal {
                        entity,
                        focus: cx.focus_handle(),
                        style,
                        height,
                        open: true,
                    },
                );
            }
            Err(error) => {
                self.notice = Some(Notice::Error(format!("Could not open terminal: {error}")))
            }
        }
        cx.notify();
    }

    /// Keep hidden terminals alive and update their colors with the app theme.
    pub(super) fn sync_terminals(&mut self, cx: &mut Context<Self>) {
        let style = terminal_style(cx);
        let obscured = self.settings.is_some()
            || self.mobile_access.pairing_visible
            || self.mobile_access.provider_prompt.is_some();
        let visible: HashSet<u64> = if obscured {
            HashSet::new()
        } else {
            self.pane_layout
                .root
                .panes()
                .into_iter()
                .filter_map(|(id, _)| {
                    let session = self.pane_session(id)?;
                    Some(
                        self.projects[session.project_index].agents[session.agent_index]
                            .config
                            .id,
                    )
                })
                .collect()
        };
        for (&id, terminal) in &mut self.terminals {
            terminal.entity.update(cx, |entity, _| {
                if terminal.style != style {
                    entity.update_style(style.clone());
                    terminal.style = style.clone();
                }
                entity.set_visible(terminal.open && visible.contains(&id));
            });
        }
    }

    pub(super) fn terminal_focused(&self, window: &Window, cx: &App) -> bool {
        self.terminals
            .values()
            .any(|terminal| terminal.focus.contains_focused(window, cx))
    }

    pub(super) fn render_terminal(&self, agent_id: u64, cx: &mut Context<Self>) -> Option<Div> {
        let terminal = self
            .terminals
            .get(&agent_id)
            .filter(|terminal| terminal.open)?;
        let palette = theme::palette(cx);
        let alive = terminal.entity.read(cx).is_alive();
        let standalone = self.session_location(agent_id).is_some_and(|session| {
            self.projects[session.project_index].agents[session.agent_index]
                .config
                .terminal
        });
        Some(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .min_w(px(0.))
                .h(px(terminal_height(
                    terminal.height,
                    self.conversation.viewport_height,
                    self.font_scale,
                )))
                .when(standalone, |panel| panel.flex_1().h_full())
                .child(
                    div()
                        .id(("terminal-divider", agent_id))
                        .when(standalone, |divider| divider.hidden())
                        .h(px(6.))
                        .w_full()
                        .flex_shrink_0()
                        .relative()
                        .cursor_ns_resize()
                        .hover(|style| style.bg(palette.color(ACCENT).opacity(0.3)))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .w_full()
                                .h(px(1.))
                                .bg(palette.color(BORDER)),
                        )
                        .on_drag(
                            TerminalResize {
                                agent_id,
                                pane_id: self.pane_id,
                            },
                            |drag, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| drag.clone())
                            },
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_1()
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(palette.color(MUTED))
                                .child(if alive {
                                    "Terminal"
                                } else {
                                    "Terminal · shell exited"
                                }),
                        )
                        .child(
                            Button::new(("terminal-restart", agent_id))
                                .ghost()
                                .compact()
                                .label("Restart")
                                .on_click(self.pane_listener(cx, move |this, _, window, cx| {
                                    this.start_terminal(agent_id, window, cx);
                                })),
                        )
                        .child(
                            Button::new(("terminal-stop", agent_id))
                                .ghost()
                                .compact()
                                .label("Stop")
                                .tooltip("Stop this terminal and its shell")
                                .on_click(self.pane_listener(cx, move |this, _, window, cx| {
                                    this.terminals.remove(&agent_id);
                                    this.focus_session_input(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(("terminal-hide", agent_id))
                                .ghost()
                                .compact()
                                .icon(IconName::Close)
                                .tooltip("Hide terminal, keeping its shell running")
                                .on_click(self.pane_listener(cx, move |this, _, window, cx| {
                                    this.toggle_terminal(agent_id, window, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .min_w(px(0.))
                        .relative()
                        .track_focus(&terminal.focus)
                        .child(terminal.entity.clone()),
                ),
        )
    }
}
