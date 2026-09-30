use super::*;

impl Render for MobileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.connected {
            let mut saved_list = div()
                .id("saved-desktops")
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child("Saved desktops"),
                );
            for connection in &self.saved_connections {
                let id = connection.id.clone();
                let rename_id = id.clone();
                saved_list = saved_list.child(
                    div()
                        .id(format!("saved-connection-{id}"))
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_lg()
                        .bg(rgb(SURFACE))
                        .child(
                            div()
                                .id(format!("open-saved-connection-{id}"))
                                .flex_1()
                                .min_w_0()
                                .px_3()
                                .py_3()
                                .cursor_pointer()
                                .child(div().truncate().child(connection.label.clone()))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_saved_connection(id.clone(), window, cx)
                                })),
                        )
                        .child(
                            div()
                                .id(format!("rename-saved-connection-{rename_id}"))
                                .px_3()
                                .py_3()
                                .text_sm()
                                .text_color(rgb(ACCENT))
                                .cursor_pointer()
                                .child("Rename")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.rename_saved_connection(rename_id.clone(), cx)
                                })),
                        ),
                );
            }
            let page = div()
                .id("locked-page")
                .size_full()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .gap_4()
                .p_4()
                .bg(rgb(BG))
                .text_color(rgb(TEXT));
            let pairing = self.pending_pairing.is_some() || (self.unlocking && !self.show_unlock);
            if pairing || self.show_unlock {
                let title = if pairing {
                    "Pair desktop".to_string()
                } else {
                    self.saved_connections
                        .iter()
                        .find(|connection| {
                            self.selected_connection.as_deref() == Some(connection.id.as_str())
                        })
                        .map(|connection| connection.label.clone())
                        .unwrap_or_else(|| "Saved desktop".into())
                };
                return page
                    .child(
                        div()
                            .id("back-to-saved-desktops")
                            .text_color(rgb(ACCENT))
                            .cursor_pointer()
                            .child("Back")
                            .when(!self.unlocking, |element| {
                                element.on_click(
                                    cx.listener(|this, _, _, cx| this.show_saved_desktops(cx)),
                                )
                            }),
                    )
                    .child(div().text_lg().child(title))
                    .when(self.unlocking, |element| {
                        element.child(
                            div()
                                .text_color(rgb(MUTED))
                                .child(self.connection_status.clone()),
                        )
                    })
                    .when_some(self.action_error.as_ref(), |element, error| {
                        element.child(div().text_color(rgb(0xe99191)).child(error.clone()))
                    })
                    .when(
                        !pairing
                            && self.protection == Protection::Phone
                            && !self.phone_unlock_available,
                        |element| {
                            element.child(
                                div()
                                    .text_color(rgb(0xe99191))
                                    .child("Phone unlock is unavailable in this browser."),
                            )
                        },
                    )
                    .when(
                        !self.unlocking
                            && self.phone_unlock_available
                            && (pairing || self.protection == Protection::Phone),
                        |element| {
                            element.child(
                                div()
                                    .id("connect-with-phone")
                                    .rounded_md()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(0x304b60))
                                    .cursor_pointer()
                                    .child(if pairing {
                                        "Pair with phone unlock"
                                    } else {
                                        "Unlock with phone"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.connect_with_phone(window, cx)
                                    })),
                            )
                        },
                    )
                    .when(
                        !self.unlocking && (pairing || self.protection == Protection::Passphrase),
                        |element| {
                            element.child(Input::new(&self.passphrase)).child(
                                div()
                                    .id("connect-with-passphrase")
                                    .rounded_md()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(0x304b60))
                                    .cursor_pointer()
                                    .child(if pairing {
                                        "Pair with passphrase"
                                    } else {
                                        "Unlock"
                                    })
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.connect(window, cx)),
                                    ),
                            )
                        },
                    )
                    .into_any_element();
            }
            return page
                .when_some(self.action_error.as_ref(), |element, error| {
                    element.child(div().text_color(rgb(0xe99191)).child(error.clone()))
                })
                .when(self.scanning, |element| {
                    element.child(div().text_color(rgb(MUTED)).child("Scanning QR code…"))
                })
                .when(!self.saved_connections.is_empty(), |element| {
                    element.child(saved_list)
                })
                .child(
                    div()
                        .id("scan-desktop-qr")
                        .rounded_md()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x304b60))
                        .cursor_pointer()
                        .child(if self.saved_connections.is_empty() {
                            "Pair a desktop"
                        } else {
                            "Pair another desktop"
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.scan_qr(window, cx))),
                )
                .into_any_element();
        }
        if self.new_session {
            let mut choices = div()
                .id("new-session-choices")
                .flex_1()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            if let Some(project) = &self.new_project {
                for (index, option) in self.agent_options.iter().enumerate() {
                    let command = option.command.clone();
                    let name = option.name.clone();
                    choices = choices.child(
                        div()
                            .id(("new-agent-option", index))
                            .rounded_lg()
                            .p_3()
                            .bg(rgb(SURFACE))
                            .cursor_pointer()
                            .child(name.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.start_new_session(command.clone(), Some(name.clone()), cx)
                            })),
                    );
                }
                choices = choices.child(
                    div()
                        .id("new-custom-agent")
                        .rounded_lg()
                        .p_3()
                        .bg(rgb(SURFACE))
                        .cursor_pointer()
                        .child("Custom ACP command")
                        .on_click(cx.listener(|this, _, _, cx| this.start_custom_session(cx))),
                );
                return div()
                    .id("new-session-page")
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .bg(rgb(BG))
                    .text_color(rgb(TEXT))
                    .child(
                        div()
                            .id("new-session-back")
                            .text_color(rgb(ACCENT))
                            .cursor_pointer()
                            .child("Back")
                            .on_click(cx.listener(|this, _, _, cx| this.back_from_new_session(cx))),
                    )
                    .child(div().text_lg().child("Choose agent"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(project.clone()),
                    )
                    .when(self.creating_session, |element| {
                        element.child(div().text_color(rgb(MUTED)).child("Starting session…"))
                    })
                    .when_some(self.action_error.as_ref(), |element, error| {
                        element.child(div().text_color(rgb(0xe99191)).child(error.clone()))
                    })
                    .child(choices)
                    .into_any_element();
            }
            for (index, project) in self.projects.iter().enumerate() {
                let path = project.path.clone();
                choices = choices.child(
                    div()
                        .id(("new-project-option", index))
                        .rounded_lg()
                        .p_3()
                        .bg(rgb(SURFACE))
                        .cursor_pointer()
                        .child(path.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_new_project(path.clone(), cx)
                        })),
                );
            }
            choices = choices.child(
                div()
                    .id("new-other-project")
                    .rounded_lg()
                    .p_3()
                    .bg(rgb(SURFACE))
                    .cursor_pointer()
                    .child("Other project…")
                    .on_click(cx.listener(|this, _, _, cx| this.prompt_new_project(cx))),
            );
            return div()
                .id("new-session-page")
                .size_full()
                .flex()
                .flex_col()
                .gap_4()
                .p_4()
                .bg(rgb(BG))
                .text_color(rgb(TEXT))
                .child(
                    div()
                        .id("new-session-back")
                        .text_color(rgb(ACCENT))
                        .cursor_pointer()
                        .child("Back")
                        .on_click(cx.listener(|this, _, _, cx| this.back_from_new_session(cx))),
                )
                .child(div().text_lg().child("Choose project"))
                .when_some(self.action_error.as_ref(), |element, error| {
                    element.child(div().text_color(rgb(0xe99191)).child(error.clone()))
                })
                .child(choices)
                .into_any_element();
        }
        let narrow = f32::from(window.viewport_size().width) < 600.;
        let mut sessions = div()
            .id("sessions")
            .flex()
            .flex_shrink_0()
            .gap_2()
            .overflow_x_scroll()
            .px_3()
            .py_2();
        for project in &self.projects {
            for agent in &project.agents {
                let id = agent.id;
                let selected = self.selected == Some(id);
                let folder = project.path.rsplit('/').next().unwrap_or(&project.path);
                sessions = sessions.child(
                    div()
                        .id(("session", id))
                        .flex_shrink_0()
                        .max_w(px(if narrow { 200. } else { 260. }))
                        .rounded_md()
                        .px_3()
                        .py_2()
                        .bg(rgb(if selected { 0x30485f } else { SURFACE }))
                        .text_color(rgb(TEXT))
                        .cursor_pointer()
                        .child(div().truncate().child(format!("{folder} · {}", agent.name)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = Some(id);
                            this.pending_new_agent = None;
                            this.conversation_scroll.scroll_to_bottom();
                            cx.notify();
                        })),
                );
            }
        }
        let mut conversation = div()
            .id("conversation")
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .overflow_y_scroll()
            .track_scroll(&self.conversation_scroll)
            .flex()
            .flex_col()
            .gap_3()
            .px_3()
            .py_4();
        if let Some(agent) = self.selected_agent() {
            if agent.has_older_messages {
                conversation = conversation.child(
                    div()
                        .text_color(rgb(MUTED))
                        .child("Showing the latest 100 messages"),
                );
            }
            for (index, message) in agent.messages.iter().enumerate() {
                let is_user = message.role == "user";
                let is_agent = message.role == "agent";
                let is_context_reset = message.role == "contextreset";
                let label = match message.role.as_str() {
                    "user" => "You",
                    "agent" => agent.name.as_str(),
                    "thought" => "Thought",
                    "tool" => "Tool",
                    "system" => "System",
                    "contextreset" => "Context reset",
                    _ => "Update",
                };
                if is_context_reset {
                    conversation = conversation.child(
                        div()
                            .id(("message", index))
                            .w_full()
                            .flex_shrink_0()
                            .py_2()
                            .text_center()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(message.text.clone()),
                    );
                    continue;
                }
                let text_id: gpui_kit::ElementId = ("mobile-message", agent.id).into();
                let content =
                    TextView::markdown((text_id, index.to_string()), message.text.clone())
                        .style(TextViewStyle {
                            paragraph_gap: rems(0.25),
                            highlight_theme: cx.theme().highlight_theme.clone(),
                            is_dark: true,
                            ..Default::default()
                        })
                        .selectable(true)
                        .text_sm()
                        .text_color(rgb(TEXT));
                conversation = conversation.child(
                    div()
                        .id(("message", index))
                        .w_full()
                        .flex_shrink_0()
                        .min_w(px(0.))
                        .flex()
                        .when(is_user, |element| element.justify_end())
                        .child(
                            div()
                                .min_w(px(0.))
                                .max_w(relative(if is_user { 0.92 } else { 1.0 }))
                                .rounded_lg()
                                .px_3()
                                .py_2()
                                .bg(rgb(if is_user {
                                    0x29465c
                                } else if is_agent {
                                    SURFACE
                                } else {
                                    0x1d2732
                                }))
                                .child(
                                    div()
                                        .mb_2()
                                        .text_xs()
                                        .text_color(rgb(if is_user || is_agent {
                                            ACCENT
                                        } else {
                                            MUTED
                                        }))
                                        .child(label.to_owned()),
                                )
                                .child(div().min_w(px(0.)).whitespace_normal().child(content)),
                        ),
                );
            }
            for (index, permission) in agent.permissions.iter().enumerate() {
                let mut card = div()
                    .rounded_lg()
                    .flex_shrink_0()
                    .min_w(px(0.))
                    .p_3()
                    .bg(rgb(0x4b382a))
                    .text_color(rgb(TEXT))
                    .child(permission.title.clone());
                if let Some(description) = &permission.description {
                    card = card.child(div().mt_2().child(description.clone()));
                }
                let mut options = div().flex().flex_wrap().gap_2().mt_3();
                for (option_index, option) in permission.options.iter().enumerate() {
                    let command = Command::Permission {
                        agent_id: agent.id,
                        request_id: permission.request_id.clone(),
                        option_id: option.id.clone(),
                    };
                    let outbound = self.outbound.clone();
                    options = options.child(
                        div()
                            .id(("option", index * 100 + option_index))
                            .rounded_md()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x304b60))
                            .cursor_pointer()
                            .child(option.label.clone())
                            .on_click(move |_, _, _| {
                                let _ = outbound.try_send(command.clone());
                            }),
                    );
                }
                conversation = conversation.child(card.child(options));
            }
        } else {
            conversation =
                conversation.child(div().text_color(rgb(MUTED)).child("No sessions yet"));
        }
        let active = self.selected_agent().is_some_and(|agent| agent.active);
        let selected = self.selected;
        let outbound = self.outbound.clone();
        div()
            .size_full()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .child(div().text_lg().child("Agentaps"))
                    .child(
                        div()
                            .id("new-session")
                            .rounded_md()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(rgb(ACCENT))
                            .cursor_pointer()
                            .child("New")
                            .on_click(cx.listener(|this, _, _, cx| this.open_new_session(cx))),
                    ),
            )
            .when(
                self.connection_status != "Connected" && !self.connection_status.is_empty(),
                |element| {
                    element.child(
                        div()
                            .px_4()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(self.connection_status.clone()),
                    )
                },
            )
            .when_some(self.action_error.as_ref(), |element, error| {
                element.child(
                    div()
                        .px_4()
                        .py_1()
                        .text_sm()
                        .text_color(rgb(0xe99191))
                        .child(error.clone()),
                )
            })
            .child(sessions)
            .child(conversation)
            .child(
                div()
                    .flex_shrink_0()
                    .p_3()
                    .bg(rgb(SURFACE))
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(Textarea::new(&self.composer)),
                    )
                    .child(
                        div()
                            .id("send")
                            .rounded_md()
                            .px_3()
                            .py_2()
                            .bg(rgb(0x304b60))
                            .cursor_pointer()
                            .child(if self.prompt_queued {
                                "Sending"
                            } else {
                                "Send"
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.send_prompt(window, cx)),
                            ),
                    )
                    .when(active, |element| {
                        element.child(
                            div()
                                .id("stop")
                                .rounded_md()
                                .px_3()
                                .py_2()
                                .bg(rgb(0x51343a))
                                .cursor_pointer()
                                .child("Stop")
                                .on_click(move |_, _, _| {
                                    if let Some(agent_id) = selected {
                                        let _ = outbound.try_send(Command::Cancel { agent_id });
                                    }
                                }),
                        )
                    }),
            )
            .into_any_element()
    }
}
