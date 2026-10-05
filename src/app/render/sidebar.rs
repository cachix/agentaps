use super::*;

fn sidebar_row() -> Div {
    div()
        .relative()
        .flex()
        .items_center()
        .gap_1()
        .px_1()
        .py(rems(0.125))
        .min_h(rems(1.5))
        .line_height(rems(1.125))
        .rounded_md()
}

fn sidebar_action(palette: theme::Palette, selected: bool) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(rems(1.375))
        .rounded_sm()
        .cursor_pointer()
        .when(selected, |action| action.bg(palette.color(SELECTED)))
        .text_color(palette.color(if selected { TEXT } else { MUTED }))
        .hover(move |style| {
            style
                .bg(palette.color(if selected { SELECTED } else { HOVER }))
                .text_color(palette.color(TEXT))
        })
}

fn sync_counts_badge(
    id: gpui_kit::ElementId,
    ahead: usize,
    behind: usize,
    palette: theme::Palette,
) -> impl IntoElement {
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
        .items_center()
        .gap_1()
        .text_size(rems(0.6875))
        .line_height(rems(1.125))
        .when(ahead > 0, |counts| {
            counts.child(
                div()
                    .text_color(palette.color(crate::diff_view::ADDED_TEXT))
                    .child(format!("↑{ahead}")),
            )
        })
        .when(behind > 0, |counts| {
            counts.child(
                div()
                    .text_color(palette.color(crate::diff_view::REMOVED_TEXT))
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
        let palette = theme::palette(cx);
        div()
            .id(id)
            .absolute()
            .right(rems(1.875))
            .top(rems(0.125))
            .h(rems(1.375))
            .flex()
            .items_center()
            .invisible()
            .group_hover(group, |style| style.visible())
            .rounded_sm()
            .bg(palette.color(HOVER))
            .px_1()
            .text_xs()
            .text_color(palette.color(TEXT))
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
        let palette = theme::palette(cx);
        let nested_sidebar = self.nested_sidebar;
        let session_query = self.sidebar_search.read(cx).value().trim().to_owned();
        let archive_view = matches!(self.view, WorkspaceView::Archive { .. });
        let new_session_view = matches!(
            self.view,
            WorkspaceView::NewSession {
                step: PickerStep::Folders
                    | PickerStep::Agents { .. }
                    | PickerStep::ChangeFolder { .. },
                ..
            }
        );
        let mut visible_sessions = 0;
        let mut project_list = div().flex().flex_col().gap(rems(0.125));
        let mut session_rows: HashMap<usize, Vec<gpui_kit::AnyElement>> = HashMap::new();
        let mut projects_with_sessions = HashSet::new();
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
            let name = agent
                .config
                .session_title()
                .unwrap_or(&agent.name)
                .to_owned();
            visible_sessions += 1;
            projects_with_sessions.insert(project_index);
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
            let show_agent_status = agent.status != Status::Idle || !agent.elicitations.is_empty();
            let sync_counts = project
                .sync_counts
                .filter(|(ahead, behind)| *ahead > 0 || *behind > 0);
            let show_sync_button = !nested_sidebar
                && sync_counts.is_some()
                && !project.agents.iter().any(|agent| agent.active_work);
            let sync_busy = self
                .sync
                .in_progress
                .contains(&(project.path.clone(), project.ssh_host.clone()));
            let row = sidebar_row()
                .id(("agent", agent_id))
                .group(row_group.clone())
                // Match the caret's visible edge inside its icon box.
                .when(nested_sidebar, |row| row.pl(rems(1.0625)))
                .pr(rems(if nested_sidebar { 1.875 } else { 0.25 }))
                .cursor_pointer()
                .when(!archived, |element| element.cursor_move())
                .bg(palette.color(if selected { SELECTED } else { SIDEBAR }))
                .hover(|style| {
                    let style = style.bg(palette.color(if selected { SELECTED } else { HOVER }));
                    if nested_sidebar {
                        style
                    } else {
                        style.pr(rems(if show_sync_button { 5.5 } else { 1.875 }))
                    }
                })
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
                    this.conversation
                        .composer
                        .update(cx, |input, cx| input.focus(window, cx));
                    cx.notify();
                }))
                .when_some(agent.config.session_title(), |row, title| {
                    let title = title.to_owned();
                    row.tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                })
                .when(!archived, |element| {
                    element
                        .on_drag(
                            AgentDrag {
                                id: agent_id,
                                project_index,
                                label: name.clone(),
                                order_index,
                            },
                            |drag: &AgentDrag, _, _, cx| cx.new(|_| drag.clone()),
                        )
                        .drag_over::<AgentDrag>(move |style, drag, _, _| {
                            if drag.id == agent_id
                                || (nested_sidebar && drag.project_index != project_index)
                            {
                                style
                            } else if drag.order_index < order_index {
                                style.border_b_2().border_color(palette.color(ACCENT))
                            } else {
                                style.border_t_2().border_color(palette.color(ACCENT))
                            }
                        })
                        .on_drop(cx.listener(move |this, drag: &AgentDrag, _, cx| {
                            this.move_agent(drag.id, agent_id, cx)
                        }))
                })
                .when(
                    show_agent_status || (!nested_sidebar && sync_counts.is_some()),
                    |row| {
                        row.child(
                            div()
                                .min_w(rems(0.875))
                                .h(rems(1.125))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_start()
                                .when(show_agent_status, |slot| {
                                    slot.child(status_badge(agent, palette))
                                })
                                .when(!show_agent_status, |slot| {
                                    slot.when_some(sync_counts, |slot, (ahead, behind)| {
                                        slot.child(sync_counts_badge(
                                            ("sync-agent", agent_id).into(),
                                            ahead,
                                            behind,
                                            palette,
                                        ))
                                    })
                                }),
                        )
                    },
                )
                .child(
                    div()
                        .when(nested_sidebar, |title| title.flex_1())
                        .when(!nested_sidebar, |title| {
                            title.flex_shrink_0().max_w(relative(0.55))
                        })
                        .min_w(px(0.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(rems(0.8125))
                        .text_color(palette.color(TEXT))
                        .child(name),
                )
                .when(!nested_sidebar, |row| {
                    let tooltip = format!("{} · {}", project.display_path(), project.branch);
                    row.child(
                        div()
                            .id(("session-branch", agent_id))
                            .flex_1()
                            .min_w(px(0.))
                            .text_right()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(rems(0.6875))
                            .text_color(palette.color(MUTED).opacity(0.75))
                            .group_hover(row_group.clone(), |style| style.invisible())
                            .child(project.branch.clone())
                            .tooltip(move |window, cx| {
                                Tooltip::new(tooltip.clone()).build(window, cx)
                            }),
                    )
                })
                .when(show_sync_button, |row| {
                    row.child(Self::render_sync_button(
                        project_index,
                        sync_action(sync_counts),
                        sync_busy,
                        ("session-sync-action", agent_id).into(),
                        row_group.clone(),
                        cx,
                    ))
                })
                .when(!agent.elicitations.is_empty(), |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .rounded_sm()
                            .bg(palette.color(SURFACE))
                            .px_1()
                            .text_xs()
                            .text_color(palette.color(STATUS_QUESTION))
                            .child(format!("{} ?", agent.elicitations.len())),
                    )
                })
                .child(
                    div()
                        .id(("archive-action", agent_id))
                        .absolute()
                        .right(rems(0.25))
                        .top(px(0.))
                        .bottom(px(0.))
                        .invisible()
                        .group_hover(row_group, |style| style.visible())
                        .flex()
                        .items_center()
                        .child(
                            sidebar_action(palette, selected)
                                .id(("archive-button", agent_id))
                                .child(
                                    Icon::new(if archived {
                                        IconName::Undo2
                                    } else {
                                        IconName::Inbox
                                    })
                                    .size(px(14.))
                                    .text_color(palette.color(TEXT)),
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
                                    this.set_archived(
                                        project_index,
                                        agent_index,
                                        !archived,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                );
            if nested_sidebar {
                session_rows
                    .entry(project_index)
                    .or_default()
                    .push(row.into_any_element());
            } else {
                project_list = project_list.child(row);
            }
        }
        let mut rendered_project = false;
        for (order_index, project_index) in self.sidebar_projects().into_iter().enumerate() {
            let project = &self.projects[project_index];
            let rows = session_rows.remove(&project_index).unwrap_or_default();
            let has_sessions = projects_with_sessions.contains(&project_index);
            if !nested_sidebar && has_sessions {
                continue;
            }
            let collapsed = self.collapsed_projects.contains(&project_index);
            if !has_sessions && (archive_view || !project.agents.is_empty()) {
                continue;
            }
            let name = project
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| project.path.display().to_string());
            if !has_sessions
                && !session_query.is_empty()
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
            visible_sessions += usize::from(!has_sessions);
            let row_group = format!("project-row-{project_index}");
            let sync_counts = project
                .sync_counts
                .filter(|(ahead, behind)| *ahead > 0 || *behind > 0);
            let action = sync_action(sync_counts);
            let sync_busy = self
                .sync
                .in_progress
                .contains(&(project.path.clone(), project.ssh_host.clone()));
            let header = sidebar_row()
                .id(("project", project_index))
                .when(nested_sidebar, |header| {
                    header.cursor_move().on_drag(
                        ProjectDrag {
                            project_index,
                            order_index,
                            label: name.clone(),
                        },
                        |drag: &ProjectDrag, _, _, cx| cx.new(|_| drag.clone()),
                    )
                })
                .group(row_group.clone())
                .hover(|style| style.bg(palette.color(HOVER)))
                .tooltip({
                    let path = project.display_path();
                    move |window, cx| Tooltip::new(path.clone()).build(window, cx)
                })
                .when(nested_sidebar, |header| {
                    header.child(
                        div()
                            .w(rems(0.375))
                            .h(px(1.))
                            .flex_shrink_0()
                            .bg(palette.color(BORDER).opacity(0.5)),
                    )
                })
                .child(
                    div()
                        .id(("project-disclosure", project_index))
                        .w(rems(0.875))
                        .h(rems(1.125))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_start()
                        .when(has_sessions, |icon| {
                            icon.cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_project(project_index, cx);
                                }))
                        })
                        .child(
                            Icon::new(if !has_sessions {
                                IconName::Folder
                            } else if collapsed {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            })
                            .size(px(14.))
                            .text_color(palette.color(MUTED)),
                        ),
                )
                .child(
                    div()
                        .id(("project-toggle", project_index))
                        .flex_shrink_1()
                        .min_w(px(0.))
                        .max_w(relative(0.55))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(rems(0.8125))
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                        .text_color(palette.color(TEXT))
                        .when(has_sessions, |name| {
                            name.cursor_move()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_project(project_index, cx);
                                }))
                        })
                        .child(name),
                )
                .when(!archive_view && has_sessions, |header| {
                    header.child(
                        sidebar_action(palette, false)
                            .id(("project-archive", project_index))
                            .invisible()
                            .group_hover(row_group.clone(), |style| style.visible())
                            .child(Icon::new(IconName::Inbox).size(px(14.)))
                            .tooltip(|window, cx| Tooltip::new("Archive project").build(window, cx))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.archive_project(project_index, window, cx);
                            })),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .group_hover(row_group.clone(), |style| style.invisible())
                        .when(nested_sidebar, |metadata| {
                            metadata.child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .h(px(1.))
                                    .bg(palette.color(BORDER).opacity(0.5)),
                            )
                        })
                        .child(
                            div()
                                .id(("project-branch", project_index))
                                .when(!nested_sidebar, |branch| branch.flex_1())
                                .min_w(px(0.))
                                .max_w(relative(1.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_right()
                                .text_size(rems(0.6875))
                                .text_color(palette.color(MUTED).opacity(0.75))
                                .child(project.branch.clone())
                                .tooltip({
                                    let branch = project.branch.clone();
                                    move |window, cx| Tooltip::new(branch.clone()).build(window, cx)
                                }),
                        ),
                )
                .when_some(sync_counts, |header, (ahead, behind)| {
                    header.child(
                        div()
                            .flex_shrink_0()
                            .group_hover(row_group.clone(), |style| style.invisible())
                            .child(sync_counts_badge(
                                ("sync-project", project_index).into(),
                                ahead,
                                behind,
                                palette,
                            )),
                    )
                })
                .child(
                    sidebar_action(palette, false)
                        .id(("project-new-session", project_index))
                        .child(Icon::new(IconName::Plus).size(px(14.)))
                        .tooltip(|window, cx| {
                            Tooltip::new("New session in this project").build(window, cx)
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_picker(
                                PickerStep::ProjectAgents { project_index },
                                window,
                                cx,
                            );
                        })),
                )
                .when(
                    sync_counts.is_some() && !project.agents.iter().any(|agent| agent.active_work),
                    |row| {
                        row.child(Self::render_sync_button(
                            project_index,
                            action,
                            sync_busy,
                            ("sync-project-action", project_index).into(),
                            row_group.clone(),
                            cx,
                        ))
                    },
                );
            project_list = project_list.child(
                div()
                    .id(("project-group", project_index))
                    .flex()
                    .flex_col()
                    .when(nested_sidebar && rendered_project, |group| group.mt_1())
                    .when(nested_sidebar, |group| {
                        group
                            .drag_over::<ProjectDrag>(move |style, drag, _, _| {
                                if drag.project_index == project_index {
                                    style
                                } else if drag.order_index < order_index {
                                    style.border_b_2().border_color(palette.color(ACCENT))
                                } else {
                                    style.border_t_2().border_color(palette.color(ACCENT))
                                }
                            })
                            .on_drop(cx.listener(move |this, drag: &ProjectDrag, _, cx| {
                                this.move_project(drag.project_index, project_index, cx);
                            }))
                    })
                    .child(header)
                    .when(has_sessions && !collapsed, |group| {
                        group
                            .gap(rems(0.125))
                            .child(div().flex().flex_col().children(rows))
                    }),
            );
            rendered_project = true;
        }
        if visible_sessions == 0 && !session_query.is_empty() {
            project_list = project_list.child(
                div()
                    .px_3()
                    .py_3()
                    .text_xs()
                    .text_color(palette.color(MUTED))
                    .child("No matching sessions"),
            );
        } else if archive_view && archived_count == 0 {
            project_list = project_list.child(
                div()
                    .px_3()
                    .py_3()
                    .text_xs()
                    .text_color(palette.color(MUTED))
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
            .bg(palette.color(SIDEBAR))
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
                    .px_2()
                    .py_1()
                    .child(project_list),
            )
            .child(
                div()
                    .p_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        sidebar_action(palette, false)
                            .id("open-settings")
                            .size(rems(1.75))
                            .child(Icon::new(IconName::Settings).size(px(14.)))
                            .tooltip(|window, cx| Tooltip::new("Settings").build(window, cx))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                            ),
                    )
                    .child(
                        sidebar_action(palette, archive_view)
                            .id("archived-toggle")
                            .size(rems(1.75))
                            .child(Icon::new(IconName::Inbox).size(px(14.)))
                            .tooltip(move |window, cx| {
                                Tooltip::new(archive_tooltip).build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_view(this.view.toggle_archive(), window, cx);
                                if this.view.displayed_session().is_some() {
                                    this.conversation
                                        .composer
                                        .update(cx, |input, cx| input.focus(window, cx));
                                } else {
                                    this.sidebar_search
                                        .update(cx, |input, cx| input.focus(window, cx));
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        sidebar_action(palette, false)
                            .id("mobile-access")
                            .size(rems(1.75))
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
                        sidebar_action(palette, new_session_view)
                            .id("quick-open")
                            .size(rems(1.75))
                            .w_auto()
                            .px_2()
                            .gap_1()
                            .text_xs()
                            .child(Icon::new(IconName::Plus).size(px(14.)).text_color(
                                palette.color(if new_session_view { TEXT } else { MUTED }),
                            ))
                            .child("New")
                            .tooltip(|window, cx| Tooltip::new("Open folder").build(window, cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_picker(PickerStep::Folders, window, cx)
                            })),
                    ),
            )
    }
}
