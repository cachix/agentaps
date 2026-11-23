use super::*;
use agentaps_control_protocol::sidebar::{score, session_search_score};
use std::cmp::Reverse;

impl SessionFilter {
    fn includes(self, agent: &Agent, archived: bool) -> bool {
        match self {
            Self::Current => !archived,
            Self::Active => {
                !archived && (agent.needs_attention || agent.active || agent.status != "idle")
            }
            Self::Archived => archived,
        }
    }

    fn empty_message(self) -> &'static str {
        match self {
            Self::Current => "No current sessions",
            Self::Active => "No active sessions",
            Self::Archived => "No archived sessions",
        }
    }
}

fn project_name(project: &Project) -> &str {
    if project.name.is_empty() {
        project
            .path
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or(&project.path)
    } else {
        &project.name
    }
}

fn session_status(agent: &Agent) -> Option<(&'static str, u32)> {
    if !agent.permissions.is_empty() {
        return Some(("Permission", 0xc8a7f4));
    }
    match agent.status.as_str() {
        "connecting" => Some(("Connecting", MUTED)),
        "working" => Some(("Working", 0xe9b978)),
        "done" => Some(("Done", 0x4ade80)),
        "error" => Some(("Error", 0xe99191)),
        _ if agent.active => Some(("Working", 0xe9b978)),
        _ if agent.needs_attention => Some(("Waiting", 0xc8a7f4)),
        _ => None,
    }
}

impl MobileView {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let search = self.session_search.read(cx).value();
        let query = search.trim();
        let searching = !query.is_empty();
        let filters = div()
            .flex()
            .gap_1()
            .rounded_lg()
            .bg(rgb(SURFACE))
            .p_1()
            .children(
                [
                    ("Current", SessionFilter::Current),
                    ("Active", SessionFilter::Active),
                    ("Archived", SessionFilter::Archived),
                ]
                .into_iter()
                .map(|(label, filter)| {
                    let selected = filter == self.session_filter;
                    div()
                        .id(label)
                        .flex_1()
                        .h(px(44.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .text_sm()
                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                        .when(selected, |tab| tab.bg(rgb(0x304b60)))
                        .cursor_pointer()
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.session_filter != filter {
                                this.session_filter = filter;
                                if this.session_search.read(cx).value().trim().is_empty() {
                                    this.sessions_scroll.set_offset(Default::default());
                                }
                                cx.notify();
                            }
                        }))
                }),
            );

        let mut list = div()
            .id("session-list")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .track_scroll(&self.sessions_scroll)
            .flex()
            .flex_col()
            .gap_3()
            .px_3()
            .pb_3();
        let mut flat_sessions = Vec::new();
        let mut groups = Vec::new();
        for project in &self.projects {
            let mut sessions = project
                .agents
                .iter()
                .map(|agent| (agent, false))
                .chain(project.archived_agents.iter().map(|agent| (agent, true)))
                .filter_map(|(agent, archived)| {
                    let rank = if searching {
                        session_search_score(
                            query,
                            project_name(project),
                            &project.path,
                            &project.branch,
                            &agent.name,
                            agent.title.as_deref(),
                        )?
                    } else {
                        self.session_filter.includes(agent, archived).then_some(0)?
                    };
                    Some((
                        Reverse(rank),
                        agent.sidebar_order.unwrap_or(usize::MAX),
                        agent,
                        archived,
                    ))
                })
                .collect::<Vec<_>>();
            if sessions.is_empty() {
                let empty_project = project.agents.is_empty() && project.archived_agents.is_empty();
                let show_empty_project = if searching {
                    [project_name(project), &project.path, &project.branch]
                        .iter()
                        .any(|value| score(query, value).is_some())
                } else {
                    self.session_filter == SessionFilter::Current
                };
                if !(empty_project && show_empty_project) {
                    continue;
                }
            } else if self.flat_sidebar {
                flat_sessions.extend(
                    sessions.into_iter().map(|(rank, order, agent, archived)| {
                        (rank, order, project, agent, archived)
                    }),
                );
                continue;
            }
            sessions.sort_by_key(|&(rank, order, _, _)| (rank, order));
            let collapsed = !searching && self.collapsed_projects.contains(&project.path);
            groups.push(self.render_project_group(project, collapsed, searching, sessions, cx));
        }
        let has_results = !flat_sessions.is_empty() || !groups.is_empty();
        if self.flat_sidebar && !flat_sessions.is_empty() {
            flat_sessions.sort_by_key(|&(rank, order, _, _, _)| (rank, order));
            list =
                list.child(
                    div()
                        .flex_shrink_0()
                        .rounded_lg()
                        .bg(rgb(SURFACE))
                        .children(flat_sessions.into_iter().map(
                            |(_, _, project, agent, archived)| {
                                render_session_row(agent, archived, searching, Some(project), cx)
                            },
                        )),
                );
        }
        list = list.children(groups);
        if !has_results && self.connection_status == "Connected" {
            list = list.child(div().px_2().py_3().text_sm().text_color(rgb(MUTED)).child(
                if searching {
                    "No matching projects or sessions"
                } else {
                    self.session_filter.empty_message()
                },
            ));
        }

        let page = self.connected_page().child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap_2()
                .p_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child("Sessions"),
                        )
                        .child(
                            div()
                                .id("new-session")
                                .h(px(44.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_3()
                                .rounded_md()
                                .bg(rgb(SURFACE))
                                .text_sm()
                                .text_color(rgb(ACCENT))
                                .cursor_pointer()
                                .child(Icon::new(IconName::Plus).size(px(16.)))
                                .child("New")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_new_session(None, window, cx)
                                })),
                        ),
                )
                .child(
                    Input::new(&self.session_search)
                        .min_h(px(44.))
                        .prefix(Icon::new(IconName::Search).size(px(16.)))
                        .cleanable(true),
                )
                .child(filters)
                .when(searching, |controls| {
                    controls.child(
                        div()
                            .px_1()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("Searching all sessions"),
                    )
                }),
        );
        self.with_status(page).child(list)
    }

    fn render_project_group(
        &self,
        project: &Project,
        collapsed: bool,
        searching: bool,
        sessions: Vec<(Reverse<i32>, usize, &Agent, bool)>,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Div {
        let has_sessions = !sessions.is_empty();
        let can_collapse = has_sessions && !searching;
        let path = project.path.clone();
        let new_project = path.clone();
        div()
            .flex_shrink_0()
            .rounded_lg()
            .bg(rgb(SURFACE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .child(
                        div()
                            .id(format!("toggle-project-{path}"))
                            .flex_1()
                            .min_w(px(0.))
                            .min_h(px(44.))
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .when(can_collapse, |header| {
                                header.cursor_pointer().on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if !this.collapsed_projects.remove(&path) {
                                            this.collapsed_projects.insert(path.clone());
                                        }
                                        cx.notify();
                                    },
                                ))
                            })
                            .child(
                                Icon::new(if !can_collapse {
                                    IconName::Folder
                                } else if collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .size(px(16.))
                                .text_color(rgb(MUTED)),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .truncate()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child(project_name(project).to_owned()),
                            )
                            .when(!project.branch.is_empty(), |header| {
                                header.child(
                                    div()
                                        .min_w(px(0.))
                                        .truncate()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(project.branch.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(format!("new-project-session-{new_project}"))
                            .w(px(44.))
                            .h(px(44.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(
                                Icon::new(IconName::Plus)
                                    .size(px(16.))
                                    .text_color(rgb(ACCENT)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_new_session(Some(new_project.clone()), window, cx);
                            })),
                    ),
            )
            .when(has_sessions && !collapsed, |group| {
                group.child(div().px_2().pb_2().children(sessions.into_iter().map(
                    |(_, _, agent, archived)| {
                        render_session_row(agent, archived, searching, None, cx)
                    },
                )))
            })
    }
}

fn render_session_row(
    agent: &Agent,
    archived: bool,
    searching: bool,
    project: Option<&Project>,
    cx: &mut Context<MobileView>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let id = agent.id;
    let status = if archived {
        searching.then_some(("Archived", MUTED))
    } else {
        session_status(agent)
    };
    div()
        .id(("session", id))
        .min_h(px(44.))
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .rounded_md()
        .hover(|row| row.bg(rgb(0x293747)))
        .cursor_pointer()
        .child(
            Icon::new(if agent.terminal {
                IconName::SquareTerminal
            } else {
                IconName::Bot
            })
            .size(px(15.))
            .text_color(rgb(MUTED)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .py_2()
                .child(div().truncate().text_sm().child(session_title(agent)))
                .when_some(project, |title, project| {
                    let project_context = if project.branch.is_empty() {
                        project_name(project).to_owned()
                    } else {
                        format!("{} · {}", project_name(project), project.branch)
                    };
                    title.child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(project_context),
                    )
                }),
        )
        .when_some(status, |row, (label, color)| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(color))
                    .child(label),
            )
        })
        .child(
            Icon::new(IconName::ChevronRight)
                .size(px(14.))
                .text_color(rgb(MUTED)),
        )
        .on_click(
            cx.listener(move |this, _, window, cx| this.open_session(id, archived, window, cx)),
        )
}
