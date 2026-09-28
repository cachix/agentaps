use super::*;
use crate::diff_view::{self, Presentation};
use gpui_component::progress::Progress;

impl Workspace {
    pub(super) fn render_diff(
        &self,
        mut panel: Div,
        project_index: usize,
        agent_index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let project = &self.projects[project_index];
        let agent = &project.agents[agent_index];
        let path = project.path.display().to_string();
        panel = panel.child(
            div()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(rgb(BORDER))
                .flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .gap_3()
                        .items_center()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(agent.name.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(path),
                        ),
                )
                .child(
                    div()
                        .id("back-to-chat")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .hover(|style| style.bg(rgb(HOVER)))
                        .child("Chat")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_diff();
                            this.composer
                                .update(cx, |input, cx| input.focus(window, cx));
                            cx.notify();
                        })),
                ),
        );
        let mut toolbar = div()
            .px_4()
            .py_2()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .text_xs()
            .text_color(rgb(MUTED));
        toolbar = toolbar
            .child("Checkout changes vs HEAD")
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .child("Shared by agents in this folder"),
            )
            .child(self.presentation_button(Presentation::Unified, "Unified", cx))
            .child(self.presentation_button(Presentation::Split, "Split", cx));
        panel = panel.child(toolbar);
        if self.diff_loading {
            panel = panel.child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Loading changes…")
                    .child(
                        Progress::new("diff-loading")
                            .loading(true)
                            .accessibility_label("Loading checkout changes"),
                    ),
            );
        }
        let body = if let Some(error) = &self.diff_error {
            div()
                .flex_1()
                .p_4()
                .text_color(rgb(ERROR_TEXT))
                .child(format!("Could not load diff: {error}"))
                .into_any_element()
        } else if self.diff_loading && self.diff_files.is_empty() {
            div().flex_1().into_any_element()
        } else if self.diff_files.is_empty() {
            div()
                .flex_1()
                .p_4()
                .text_color(rgb(MUTED))
                .child("No changes in this checkout")
                .into_any_element()
        } else {
            let view = cx.entity().clone();
            let rows = self.diff_rows.clone();
            div()
                .flex_1()
                .min_h(px(0.))
                .relative()
                .child(
                    div().id("diff-scroll").size_full().child(
                        gpui::list(self.diff_list.clone(), move |index, _, cx| {
                            view.update(cx, |this, cx| this.render_diff_list_row(&rows[index], cx))
                        })
                        .w_full()
                        .h_full(),
                    ),
                )
                .vertical_scrollbar(&self.diff_list)
                .into_any_element()
        };
        panel.child(body)
    }

    fn render_diff_list_row(&self, row: &DiffListRow, cx: &mut Context<Self>) -> gpui::AnyElement {
        match row {
            DiffListRow::Summary {
                files,
                added,
                removed,
            } => {
                let summary = format!(
                    "{files} {} changed, {added} {}(+), {removed} {}(-)",
                    if *files == 1 { "file" } else { "files" },
                    if *added == 1 {
                        "insertion"
                    } else {
                        "insertions"
                    },
                    if *removed == 1 {
                        "deletion"
                    } else {
                        "deletions"
                    },
                );
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .font_family("monospace")
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(summary)
                    .into_any_element()
            }
            DiffListRow::File {
                path,
                note,
                added,
                removed,
                bar_width,
                added_width,
                expanded,
            } => {
                let selected_path = path.clone();
                let visible_note = if *expanded { None } else { note.as_ref() };
                div()
                    .id(format!("diff-file-{path}"))
                    .min_h(px(44.))
                    .px_4()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_diff_file(selected_path.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(MUTED))
                            .child(if *expanded { "▾" } else { "▸" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .font_family("monospace")
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .child(path.clone()),
                            )
                            .when_some(visible_note, |element, note| {
                                element.child(
                                    div()
                                        .truncate()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(note.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family("monospace")
                            .text_xs()
                            .text_color(rgb(diff_view::ADDED_TEXT))
                            .child(format!("+{added}")),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family("monospace")
                            .text_xs()
                            .text_color(rgb(diff_view::REMOVED_TEXT))
                            .child(format!("-{removed}")),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(96.))
                            .h(px(8.))
                            .flex()
                            .rounded_sm()
                            .overflow_hidden()
                            .child(
                                div()
                                    .w(px(*added_width))
                                    .h_full()
                                    .bg(rgb(diff_view::ADDED_TEXT)),
                            )
                            .child(
                                div()
                                    .w(px(*bar_width - *added_width))
                                    .h_full()
                                    .bg(rgb(diff_view::REMOVED_TEXT)),
                            ),
                    )
                    .into_any_element()
            }
            DiffListRow::Content(row) => diff_view::render_row(row),
        }
    }

    fn presentation_button(
        &self,
        presentation: Presentation,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.diff_presentation == presentation;
        div()
            .id(label)
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(rgb(if selected { SELECTED } else { SURFACE }))
            .text_color(rgb(if selected { TEXT } else { MUTED }))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_diff_presentation(presentation, cx);
            }))
    }
}
