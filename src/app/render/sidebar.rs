use super::*;

fn sync_counts_badge(id: gpui_kit::ElementId, ahead: usize, behind: usize) -> impl IntoElement {
    let tooltip = match (ahead, behind) {
        (0, behind) => format!("{behind} behind upstream"),
        (ahead, 0) => format!("{ahead} ahead of upstream"),
        (ahead, behind) => format!(
            "{ahead} ahead, {behind} behind upstream. Resolve diverged commits in Git before syncing"
        ),
    };
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_start()
        .text_size(px(9.))
        .line_height(px(10.))
        .when(ahead > 0, |counts| {
            counts.child(
                div()
                    .text_color(rgb(crate::diff_view::ADDED_TEXT))
                    .child(format!("↑{ahead}")),
            )
        })
        .when(behind > 0, |counts| {
            counts.child(
                div()
                    .text_color(rgb(crate::diff_view::REMOVED_TEXT))
                    .child(format!("↓{behind}")),
            )
        })
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
}

fn sync_action(counts: Option<(usize, usize)>) -> Option<crate::git_sync::SyncAction> {
    match counts {
        Some((ahead, 0)) if ahead > 0 => Some(crate::git_sync::SyncAction::Push),
        Some((0, behind)) if behind > 0 => Some(crate::git_sync::SyncAction::Pull),
        _ => None,
    }
}

impl Workspace {
    fn render_sync_button(
        project_index: usize,
        action: Option<crate::git_sync::SyncAction>,
        busy: bool,
        id: gpui_kit::ElementId,
        group: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .absolute()
            .right(px(30.))
            .top(px(2.))
            .invisible()
            .group_hover(group, |style| style.visible())
            .rounded_sm()
            .bg(rgb(ACCENT_SURFACE))
            .px_2()
            .py_1()
            .text_xs()
            .text_color(rgb(TEXT))
            .when(action.is_some() && !busy, |button| button.cursor_pointer())
            .child(if busy {
                "Syncing…"
            } else {
                match action {
                    Some(crate::git_sync::SyncAction::Push) => "Push",
                    Some(crate::git_sync::SyncAction::Pull) => "Pull",
                    None => "Diverged",
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(action.filter(|_| !busy), |button, action| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.start_git_sync(project_index, action, cx);
                }))
            })
    }

    pub(super) fn render_sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let session_query = self.sidebar_search.read(cx).value().trim().to_owned();
        let archive_view = matches!(self.view, WorkspaceView::Archive { .. });
        let new_session_view = matches!(self.view, WorkspaceView::NewSession { .. });
        let mut visible_sessions = 0;
        let mut project_list = div().flex().flex_col().gap_1();
        let mut archived_list = div().flex().flex_col().gap_1();
        let archived_count = self
            .projects
            .iter()
            .flat_map(|project| &project.agents)
            .filter(|agent| agent.config.archived)
            .count();
        for (index, location) in self.sidebar_results(&session_query).into_iter().enumerate() {
            let SessionLocation {
                project_index,
                agent_index,
            } = location;
            let project = &self.projects[project_index];
            let agent = &project.agents[agent_index];
            let archived = agent.config.archived;
            let name = project
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| project.path.display().to_string());
            visible_sessions += 1;
            let selected = if session_query.is_empty() {
                self.view.highlighted_session() == Some(location)
            } else {
                index == self.sidebar_selection
            };
            let agent_id = agent.config.id;
            let order_index = self
                .sidebar_order
                .iter()
                .position(|id| *id == agent_id)
                .unwrap_or(usize::MAX);
            let row_group = format!("agent-row-{agent_id}");
            let project_working = project.agents.iter().any(|agent| agent.active_work);
            let sync_counts = project
                .sync_counts
                .filter(|(ahead, behind)| *ahead > 0 || *behind > 0);
            let show_sync_button = sync_counts.is_some()
                && !project_working
                && !matches!(agent.status, Status::Working | Status::Done);
            let action = sync_action(sync_counts);
            let sync_busy = self
                .sync_in_progress
                .contains(&(project.path.clone(), project.ssh_host.clone()));
            let row = div()
                .id(("agent", agent_id))
                .group(row_group.clone())
                .relative()
                .flex()
                .items_center()
                .gap_1()
                .pl_1()
                .pr_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .when(!archived, |element| element.cursor_move())
                .bg(rgb(if selected { SELECTED } else { SIDEBAR }))
                .hover(|style| style.bg(rgb(HOVER)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if archived {
                        this.set_archived(project_index, agent_index, false, window, cx);
                        return;
                    }
                    this.sidebar_selection = index;
                    this.set_view(
                        WorkspaceView::Conversation(SessionLocation {
                            project_index,
                            agent_index,
                        }),
                        window,
                        cx,
                    );
                    this.composer
                        .update(cx, |input, cx| input.focus(window, cx));
                    cx.notify();
                }))
                .when_some(agent.config.title.as_ref(), |row, title| {
                    let title = title.clone();
                    row.tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                })
                .when(!archived, |element| {
                    element
                        .on_drag(
                            AgentDrag {
                                id: agent_id,
                                label: name.clone(),
                                order_index,
                            },
                            |drag: &AgentDrag, _, _, cx| cx.new(|_| drag.clone()),
                        )
                        .drag_over::<AgentDrag>(move |style, drag, _, _| {
                            if drag.id == agent_id {
                                style
                            } else if drag.order_index < order_index {
                                style.border_b_2().border_color(rgb(ACCENT))
                            } else {
                                style.border_t_2().border_color(rgb(ACCENT))
                            }
                        })
                        .on_drop(cx.listener(move |this, drag: &AgentDrag, _, cx| {
                            this.move_agent(drag.id, agent_id, cx)
                        }))
                })
                .child(
                    div()
                        .w(px(16.))
                        .h(px(20.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_start()
                        .child(status_badge(agent)),
                )
                .when_some(sync_counts, |row, (ahead, behind)| {
                    row.child(
                        div()
                            .w(px(16.))
                            .h(px(20.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(sync_counts_badge(("sync", agent_id).into(), ahead, behind)),
                    )
                })
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(relative(0.55))
                        .truncate()
                        .text_sm()
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .text_right()
                        .group_hover(row_group.clone(), |style| style.invisible())
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(project.branch.clone()),
                )
                .when(!agent.elicitations.is_empty(), |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .rounded_sm()
                            .bg(rgb(SURFACE))
                            .px_1()
                            .text_xs()
                            .text_color(rgb(STATUS_QUESTION))
                            .child(format!("{} ?", agent.elicitations.len())),
                    )
                })
                .when(show_sync_button, |row| {
                    row.child(Self::render_sync_button(
                        project_index,
                        action,
                        sync_busy,
                        ("sync-action", agent_id).into(),
                        row_group.clone(),
                        cx,
                    ))
                })
                .child(
                    div()
                        .id(("archive-action", agent_id))
                        .absolute()
                        .right(px(4.))
                        .top(px(2.))
                        .invisible()
                        .group_hover(row_group, |style| style.visible())
                        .rounded_sm()
                        .bg(rgb(HOVER))
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(TEXT))
                        .cursor_pointer()
                        .child(
                            Icon::new(if archived {
                                IconName::Undo2
                            } else {
                                IconName::Inbox
                            })
                            .size(px(14.))
                            .text_color(rgb(TEXT)),
                        )
                        .tooltip(move |window, cx| {
                            Tooltip::new(if archived {
                                "Restore session"
                            } else {
                                "Archive session"
                            })
                            .build(window, cx)
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.set_archived(project_index, agent_index, !archived, window, cx);
                        })),
                );
            if archived {
                archived_list = archived_list.child(row);
            } else {
                project_list = project_list.child(row);
            }
        }
        for (project_index, project) in self.projects.iter().enumerate() {
            if archive_view {
                break;
            }
            if project.agents.is_empty() {
                let name = project
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| project.path.display().to_string());
                if !session_query.is_empty()
                    && [
                        name.as_str(),
                        project.path.to_str().unwrap_or_default(),
                        project.branch.as_str(),
                    ]
                    .iter()
                    .all(|value| score(&session_query, value).is_none())
                {
                    continue;
                }
                visible_sessions += 1;
                let row_group = format!("project-row-{project_index}");
                let sync_counts = project
                    .sync_counts
                    .filter(|(ahead, behind)| *ahead > 0 || *behind > 0);
                let action = sync_action(sync_counts);
                let sync_busy = self
                    .sync_in_progress
                    .contains(&(project.path.clone(), project.ssh_host.clone()));
                project_list = project_list.child(
                    div()
                        .id(("project", project_index))
                        .group(row_group.clone())
                        .relative()
                        .flex()
                        .items_center()
                        .gap_1()
                        .pl_1()
                        .pr_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(HOVER)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_picker(PickerStep::Agents { project_index }, window, cx);
                        }))
                        .child(
                            div()
                                .w(px(16.))
                                .h(px(20.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_start()
                                .when_some(sync_counts, |slot, (ahead, behind)| {
                                    slot.child(sync_counts_badge(
                                        ("sync-project", project_index).into(),
                                        ahead,
                                        behind,
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .max_w(relative(0.55))
                                .truncate()
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(name),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_right()
                                .group_hover(row_group.clone(), |style| style.invisible())
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(project.branch.clone()),
                        )
                        .child(div().text_xs().text_color(rgb(ACCENT)).child("add"))
                        .when(sync_counts.is_some(), |row| {
                            row.child(Self::render_sync_button(
                                project_index,
                                action,
                                sync_busy,
                                ("sync-project-action", project_index).into(),
                                row_group.clone(),
                                cx,
                            ))
                        }),
                );
            }
        }
        if archive_view {
            project_list = archived_list;
        }
        if visible_sessions == 0 && !session_query.is_empty() {
            project_list = project_list.child(
                div()
                    .px_3()
                    .py_3()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("No matching sessions"),
            );
        } else if archive_view && archived_count == 0 {
            project_list = project_list.child(
                div()
                    .px_3()
                    .py_3()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("No archived sessions"),
            );
        }
        let viewport_width = f32::from(window.viewport_size().width);
        let sidebar_width = (viewport_width * self.sidebar_fraction).max(180.);
        let archive_tooltip = if archive_view {
            "Show active sessions"
        } else {
            "Show archived sessions"
        };
        div()
            .w(px(sidebar_width))
            .min_w(px(180.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SIDEBAR))
            .child(
                div()
                    .p_2()
                    .child(Input::new(&self.sidebar_search).cleanable(true)),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .pl_1()
                    .pr_3()
                    .py_3()
                    .child(project_list),
            )
            .child(
                div()
                    .p_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id("archived-toggle")
                            .cursor_pointer()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if archive_view { ACCENT } else { SIDEBAR }))
                            .bg(rgb(if archive_view { SELECTED } else { SIDEBAR }))
                            .px_2()
                            .py_2()
                            .flex()
                            .items_center()
                            .text_color(rgb(if archive_view { TEXT } else { MUTED }))
                            .hover(|style| style.bg(rgb(DROP_TARGET)).text_color(rgb(TEXT)))
                            .child(Icon::new(IconName::Inbox).size(px(14.)))
                            .tooltip(move |window, cx| {
                                Tooltip::new(archive_tooltip).build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_view(this.view.toggle_archive(), window, cx);
                                if this.view.displayed_session().is_some() {
                                    this.composer
                                        .update(cx, |input, cx| input.focus(window, cx));
                                } else {
                                    this.sidebar_search
                                        .update(cx, |input, cx| input.focus(window, cx));
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("mobile-access")
                            .cursor_pointer()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(SIDEBAR))
                            .px_2()
                            .py_2()
                            .flex()
                            .items_center()
                            .text_color(rgb(MUTED))
                            .hover(|style| style.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                            .child(Icon::empty().path("icons/mobile.svg").size(px(14.)))
                            .tooltip(|window, cx| Tooltip::new("Mobile access").build(window, cx))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.show_mobile_link(window, cx)
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("quick-open")
                            .cursor_pointer()
                            .flex_shrink_0()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if new_session_view { ACCENT } else { SIDEBAR }))
                            .bg(rgb(if new_session_view { SELECTED } else { SIDEBAR }))
                            .px_2()
                            .py_2()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(rgb(if new_session_view { TEXT } else { MUTED }))
                            .hover(|style| style.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                            .child(
                                Icon::new(IconName::Plus)
                                    .size(px(14.))
                                    .text_color(rgb(if new_session_view { TEXT } else { MUTED })),
                            )
                            .child("New")
                            .tooltip(|window, cx| Tooltip::new("Open folder").build(window, cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_picker(PickerStep::Folders, window, cx)
                            })),
                    ),
            )
    }
}
