use super::*;
use crate::git_clone::{self, Task, Update};
use gpui_kit::Div;

pub(super) struct ClonePicker {
    pub name: Entity<InputState>,
    pub parent: Option<PathBuf>,
    pub suggested_name: String,
    pub task: Option<Task>,
    pub progress: String,
    pub error: Option<String>,
}

impl ClonePicker {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Repository folder name")),
            parent: None,
            suggested_name: String::new(),
            task: None,
            progress: String::new(),
            error: None,
        }
    }
}

impl Workspace {
    pub(super) fn subscribe_clone_name(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions.push(cx.subscribe_in(
            input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                let id = if this.picker.clone.name.entity_id() == input.entity_id() {
                    Some(this.pane_id)
                } else {
                    this.inactive_panes.iter().find_map(|(&id, state)| {
                        (state.picker.clone.name.entity_id() == input.entity_id()).then_some(id)
                    })
                };
                let Some(id) = id else {
                    return;
                };
                if !matches!(
                    event,
                    InputEvent::Focus | InputEvent::Change | InputEvent::PressEnter { .. }
                ) {
                    return;
                }
                this.activate_pane(id, cx);
                if matches!(
                    this.view,
                    WorkspaceView::NewSession {
                        step: PickerStep::CloneRepository,
                        ..
                    }
                ) {
                    match event {
                        InputEvent::PressEnter { .. } => this.start_clone(cx),
                        InputEvent::Change => {
                            this.picker.clone.error = None;
                            cx.notify();
                        }
                        _ => {}
                    }
                }
            },
        ));
    }

    pub(super) fn open_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker.clone.error = None;
        self.open_picker(PickerStep::CloneRepository, window, cx);
        self.update_clone_name(window, cx);
    }

    pub(super) fn update_clone_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let suggested =
            git_clone::repository_name(&self.picker.input.read(cx).value()).unwrap_or_default();
        let current = self.picker.clone.name.read(cx).value().to_string();
        if current.is_empty() || current == self.picker.clone.suggested_name {
            self.picker.clone.name.update(cx, |input, cx| {
                input.set_value(suggested.clone(), window, cx)
            });
        }
        self.picker.clone.suggested_name = suggested;
        self.picker.clone.error = None;
    }

    pub(super) fn choose_clone_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.folder_dialog_open || self.picker.clone.task.is_some() {
            return;
        }
        self.picker.folder_dialog_open = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Clone Here".into()),
        });
        let pane_id = self.pane_id;
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.with_pane(pane_id, cx, |this, cx| {
                    this.picker.folder_dialog_open = false;
                    if !matches!(
                        this.view,
                        WorkspaceView::NewSession {
                            step: PickerStep::CloneRepository,
                            ..
                        }
                    ) {
                        return;
                    }
                    match result {
                        Ok(Ok(Some(paths))) => {
                            this.picker.clone.parent = paths.into_iter().next();
                            this.picker.clone.error = None;
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(error)) => {
                            this.picker.clone.error =
                                Some(format!("Could not choose a destination: {error}"));
                        }
                        Err(error) => {
                            this.picker.clone.error =
                                Some(format!("Folder chooser closed: {error}"));
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(super) fn start_clone(&mut self, cx: &mut Context<Self>) {
        if self.picker.clone.task.is_some() {
            return;
        }
        let url = self.picker.input.read(cx).value().trim().to_owned();
        let name = self.picker.clone.name.read(cx).value().trim().to_owned();
        let validation =
            git_clone::repository_name(&url).and_then(|_| git_clone::validate_name(&name));
        if let Err(error) = validation {
            self.picker.clone.error = Some(error);
        } else if let Some(parent) = self.picker.clone.parent.clone() {
            self.picker.clone.error = None;
            self.picker.clone.progress = "Connecting to repository…".into();
            self.picker.clone.task = Some(Task::start(url, parent, name));
        } else {
            self.picker.clone.error = Some("Choose a parent folder for the clone.".into());
        }
        cx.notify();
    }

    pub(super) fn cancel_clone(&mut self, cx: &mut Context<Self>) {
        if let Some(task) = &self.picker.clone.task {
            task.cancel();
            self.picker.clone.progress = "Cancelling clone…".into();
            cx.notify();
        }
    }

    pub(super) fn poll_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let updates = self
            .picker
            .clone
            .task
            .as_ref()
            .map(|task| task.updates.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for update in updates {
            match update {
                Update::Progress(progress) => self.picker.clone.progress = progress,
                Update::Finished(result) => {
                    self.picker.clone.task = None;
                    match result {
                        Ok(path) => {
                            let background = self.pane_id != self.pane_layout.focused;
                            let focus = window.focused(cx);
                            self.select_folder(path, window, cx);
                            if background {
                                if let Some(focus) = focus {
                                    window.focus(&focus, cx);
                                } else {
                                    window.blur(cx);
                                }
                            }
                        }
                        Err(error) => {
                            self.picker.clone.error = Some(error);
                        }
                    }
                }
            }
            cx.notify();
        }
    }
}

impl Workspace {
    pub(super) fn render_clone_picker(&self, chat: Div, cx: &mut Context<Self>) -> Div {
        let palette = theme::palette(cx);
        let clone = &self.picker.clone;
        let running = clone.task.is_some();
        let destination = clone.parent.as_ref().map(|parent| {
            parent
                .join(clone.name.read(cx).value().trim())
                .display()
                .to_string()
        });
        chat.child(
            div()
                .id("clone-page")
                .flex_1()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .items_center()
                .p_6()
                .child(
                    div()
                        .w_full()
                        .max_w(px(680.))
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(palette.color(ACCENT))
                                        .child("NEW SESSION"),
                                )
                                .child(Button::new("clone-back").ghost().label("Back").on_click(
                                    self.pane_listener(cx, |this, _, window, cx| {
                                        this.back_from_picker(window, cx)
                                    }),
                                )),
                        )
                        .child(
                            div()
                                .text_2xl()
                                .text_color(palette.color(TEXT))
                                .child("Clone a repository"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(palette.color(MUTED))
                                .child("Clone into a new local folder, then choose an agent."),
                        )
                        .child(div().text_sm().child("Repository URL"))
                        .child(Input::new(&self.picker.input).disabled(running))
                        .child(div().text_sm().child("Folder name"))
                        .child(Input::new(&clone.name).disabled(running))
                        .child(
                            Button::new("clone-parent")
                                .outline()
                                .label("Choose parent folder…")
                                .disabled(running || self.picker.folder_dialog_open)
                                .on_click(self.pane_listener(cx, |this, _, window, cx| {
                                    this.choose_clone_parent(window, cx)
                                })),
                        )
                        .when_some(destination, |element, destination| {
                            element.child(
                                div()
                                    .text_sm()
                                    .text_color(palette.color(MUTED))
                                    .child(destination),
                            )
                        })
                        .when(running, |element| {
                            element.child(
                                div()
                                    .text_sm()
                                    .text_color(palette.color(MUTED))
                                    .child(clone.progress.clone()),
                            )
                        })
                        .when_some(clone.error.clone(), |element, error| {
                            element.child(
                                div()
                                    .text_sm()
                                    .text_color(palette.color(STATUS_ERROR))
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(
                                    Button::new("start-clone")
                                        .primary()
                                        .label("Clone repository")
                                        .disabled(
                                            running
                                                || clone.parent.is_none()
                                                || self
                                                    .picker
                                                    .input
                                                    .read(cx)
                                                    .value()
                                                    .trim()
                                                    .is_empty()
                                                || clone.name.read(cx).value().trim().is_empty(),
                                        )
                                        .on_click(self.pane_listener(cx, |this, _, _, cx| {
                                            this.start_clone(cx)
                                        })),
                                )
                                .when(running, |element| {
                                    element.child(
                                        Button::new("cancel-clone")
                                            .ghost()
                                            .label("Cancel")
                                            .on_click(self.pane_listener(cx, |this, _, _, cx| {
                                                this.cancel_clone(cx)
                                            })),
                                    )
                                }),
                        ),
                ),
        )
    }
}
