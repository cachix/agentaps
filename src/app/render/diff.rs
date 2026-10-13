use super::*;
use crate::diff_view::{self, Presentation};
use gpui_kit::component::{Sizable, spinner::Spinner};

impl Workspace {
    pub(in crate::app) fn render_diff(&self, panel: Div, cx: &mut Context<Self>) -> Div {
        let palette = theme::palette(cx);
        let (added, removed) = self
            .diff
            .file_stats
            .iter()
            .copied()
            .fold((0, 0), |total, count| {
                (total.0 + count.0, total.1 + count.1)
            });
        let files = self.diff.files.len();
        let toolbar = div()
            .px_4()
            .py_2()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(palette.color(BORDER))
            .text_xs()
            .text_color(palette.color(MUTED))
            .child(
                div()
                    .id("diff-comparison")
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(palette.color(TEXT))
                    .child("Checkout vs HEAD")
                    .tooltip(|window, cx| {
                        Tooltip::new("Checkout changes compared with HEAD").build(window, cx)
                    }),
            )
            .child(
                div()
                    .id("diff-loading-spinner")
                    .size(px(16.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.diff.loading, |slot| {
                        slot.child(Spinner::new().small().color(palette.color(MUTED)))
                            .tooltip(|window, cx| {
                                Tooltip::new("Loading checkout changes").build(window, cx)
                            })
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .font_family("monospace")
                    .child(format!(
                        "{files} {}",
                        if files == 1 { "file" } else { "files" }
                    ))
                    .child(
                        div()
                            .text_color(palette.color(diff_view::ADDED_TEXT))
                            .child(format!("+{added}")),
                    )
                    .child(
                        div()
                            .text_color(palette.color(diff_view::REMOVED_TEXT))
                            .child(format!("-{removed}")),
                    ),
            )
            .child(self.presentation_button(Presentation::Unified, "Unified", cx))
            .child(self.presentation_button(Presentation::Split, "Split", cx))
            .child(
                div()
                    .id("close-diff")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_color(palette.color(TEXT))
                    .hover(|style| style.bg(palette.color(HOVER)))
                    .child("Close")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_diff();
                        this.conversation
                            .composer
                            .update(cx, |input, cx| input.focus(window, cx));
                        cx.notify();
                    })),
            );
        let panel = panel.child(toolbar);
        let body = if let Some(error) = &self.diff.error {
            div()
                .flex_1()
                .p_4()
                .text_color(palette.color(ERROR_TEXT))
                .child(format!("Could not load diff: {error}"))
                .into_any_element()
        } else if self.diff.loading && self.diff.files.is_empty() {
            div().flex_1().into_any_element()
        } else if self.diff.files.is_empty() {
            div()
                .flex_1()
                .p_4()
                .text_color(palette.color(MUTED))
                .child("No changes in this checkout")
                .into_any_element()
        } else {
            let notes: HashMap<_, _> = self
                .diff
                .files
                .iter()
                .filter_map(|file| {
                    file.note
                        .as_ref()
                        .map(|note| (file.path.clone(), note.clone()))
                })
                .collect();
            checkout_diff(&self.diff.viewer, self.diff.presentation, notes).into_any_element()
        };
        panel.child(body)
    }

    fn presentation_button(
        &self,
        presentation: Presentation,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = theme::palette(cx);
        let selected = self.diff.presentation == presentation;
        div()
            .id(label)
            .px_2()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .bg(palette.color(if selected { SELECTED } else { SURFACE }))
            .text_color(palette.color(if selected { TEXT } else { MUTED }))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_diff_presentation(presentation, cx);
            }))
    }
}

fn checkout_diff(
    viewer: &Entity<gpui_kit::component::diff::DiffState>,
    presentation: Presentation,
    notes: HashMap<String, String>,
) -> gpui_kit::component::diff::Diff {
    use gpui_kit::base::TestSupportExt;
    let header_viewer = viewer.clone();
    gpui_kit::component::diff::Diff::new(viewer)
        .soft_wrap(presentation == Presentation::Split)
        .size_full()
        .flex_1()
        .min_h(px(0.))
        .render_header(move |file, _, cx| {
            let palette = theme::palette(cx);
            let viewer = header_viewer.clone();
            let path = file.path().clone();
            div()
                .id("diff-file-header-content")
                .test_support()
                .role(gpui_kit::accesskit::Role::Button)
                .aria_label(format!("Toggle diff for {path}"))
                .w_full()
                .cursor_pointer()
                .hover(|style| style.bg(palette.color(HOVER)))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    viewer.update(cx, |state, cx| {
                        let collapsed = state.is_file_collapsed(&path);
                        state.set_file_collapsed(&path, !collapsed, cx);
                    });
                })
                .flex()
                .items_center()
                .gap_3()
                .min_w(px(0.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w(px(0.))
                        .max_w(px(240.))
                        .child(
                            div()
                                .truncate()
                                .font_weight(gpui_kit::FontWeight::MEDIUM)
                                .child(file.path().clone()),
                        )
                        .when_some(notes.get(file.path().as_ref()), |this, note| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(palette.color(MUTED))
                                    .child(note.clone()),
                            )
                        }),
                )
                .child(
                    div()
                        .id("diff-file-counts")
                        .test_support()
                        .role(gpui_kit::accesskit::Role::Label)
                        .aria_label(format!(
                            "{} added lines, {} removed lines",
                            file.additions(),
                            file.deletions()
                        ))
                        .flex()
                        .flex_shrink_0()
                        .gap_2()
                        .font_family("monospace")
                        .text_xs()
                        .child(
                            div()
                                .text_color(palette.color(diff_view::ADDED_TEXT))
                                .child(format!("+{}", file.additions())),
                        )
                        .child(
                            div()
                                .text_color(palette.color(diff_view::REMOVED_TEXT))
                                .child(format!("-{}", file.deletions())),
                        ),
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::diff::{DiffFile, DiffState};

    struct Preview {
        state: Entity<DiffState>,
        presentation: Presentation,
        _subscription: Subscription,
    }

    impl Render for Preview {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            checkout_diff(&self.state, self.presentation, HashMap::new())
        }
    }

    #[gpui_kit::test]
    fn split_keeps_both_sources_and_file_counts_in_view(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            theme::apply(cx);
        });
        let long = "long source ".repeat(100);
        let patch = format!(
            "--- a/file.rs\n+++ b/file.rs\n@@ -1,2 +1,2 @@\n-before\n-{long}\n+after\n+new {long}\n"
        );
        let files = DiffFile::parse(&patch).unwrap();
        let state = cx.new(|cx| DiffState::new(files, cx).with_mode(Presentation::Unified.mode()));
        let (preview, cx) = cx.add_window_view(|_, cx| Preview {
            _subscription: cx.observe(&state, |_, _, cx| cx.notify()),
            state: state.clone(),
            presentation: Presentation::Unified,
        });
        cx.simulate_resize(size(px(500.), px(600.)));
        for presentation in [Presentation::Unified, Presentation::Split] {
            cx.update(|_, cx| {
                preview.update(cx, |view, cx| {
                    view.presentation = presentation;
                    view.state.update(cx, |state, cx| {
                        state.set_mode(presentation.mode(), cx);
                        state.scroll_to_file("file.rs", cx);
                    });
                    cx.notify();
                });
            });
            for _ in 0..3 {
                cx.update(|window, cx| {
                    window.draw(cx).clear(cx);
                });
            }
            cx.update(|window, _| {
                let snapshots = gpui_kit::base::test_support::snapshots(window);
                let source = |label: &str| {
                    snapshots
                        .iter()
                        .find(|element| element.label() == Some(label))
                        .unwrap_or_else(|| {
                            panic!("missing {label} in {presentation:?}: {snapshots:?}")
                        })
                };
                let old = source("before").bounds();
                let new = source("after").bounds();
                let counts = source("2 added lines, 2 removed lines");
                assert!(counts.visible());
                assert!(counts.bounds().right() <= window.viewport_size().width);
                if presentation == Presentation::Split {
                    assert_eq!(old.top(), new.top());
                    assert!(old.left() < new.left());
                    assert!(
                        new.right() <= window.viewport_size().width,
                        "modified source must stay inside the split viewport: {new:?}"
                    );
                } else {
                    assert!(old.top() < new.top());
                }
            });
            let row_point = cx.update(|window, _| {
                let header = gpui_kit::base::test_support::snapshots(window)
                    .into_iter()
                    .find(|element| element.label() == Some("Toggle diff for file.rs"))
                    .unwrap();
                gpui_kit::point(
                    window.viewport_size().width - px(24.),
                    header.bounds().center().y,
                )
            });
            // Blank space after the filename and counts must also toggle the file.
            cx.simulate_click(row_point, Default::default());
            cx.update(|_, cx| assert!(state.read(cx).is_file_collapsed("file.rs")));
            for _ in 0..3 {
                cx.update(|window, cx| {
                    window.draw(cx).clear(cx);
                });
            }
            let chevron_point = cx.update(|window, _| {
                gpui_kit::base::test_support::snapshots(window)
                    .into_iter()
                    .find(|element| {
                        element.path().last() == Some(&gpui_kit::ElementId::from("collapse-file"))
                    })
                    .unwrap()
                    .bounds()
                    .center()
            });
            cx.simulate_click(chevron_point, Default::default());
            cx.update(|_, cx| {
                assert!(
                    !state.read(cx).is_file_collapsed("file.rs"),
                    "the chevron must toggle exactly once"
                )
            });
        }
    }
}
