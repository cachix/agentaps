use super::*;
mod render;

pub(super) struct MobileView {
    client_cancel: Option<Sender<()>>,
    projects: Vec<Project>,
    agent_options: Vec<AgentOption>,
    selected: Option<u64>,
    pending_new_agent: Option<u64>,
    new_session: bool,
    new_project: Option<String>,
    creating_session: bool,
    saved_connections: Vec<SavedConnection>,
    selected_connection: Option<String>,
    show_unlock: bool,
    conversation_scroll: ScrollHandle,
    composer: Entity<TextareaState>,
    passphrase: Entity<InputState>,
    outbound: Sender<Command>,
    incoming: Receiver<Command>,
    pending_pairing: Option<(EndpointId, String)>,
    protection: Protection,
    phone_unlock_available: bool,
    connected: bool,
    unlocking: bool,
    scanning: bool,
    generation: u64,
    connection_status: String,
    action_error: Option<String>,
    prompt_queued: bool,
    _subscriptions: Vec<gpui_kit::Subscription>,
}

impl MobileView {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        remote: Option<(EndpointId, String)>,
    ) -> Self {
        let composer = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask your agent…"));
        let passphrase = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(if remote.is_some() {
                    "Choose a passphrase (15+ characters)"
                } else {
                    "Unlock passphrase"
                })
                .masked(true)
        });
        let subscription = cx.subscribe_in(&composer, window, |this, _, event, window, cx| {
            if matches!(
                event,
                InputEvent::PressEnter {
                    secondary: false,
                    shift: false
                }
            ) {
                this.send_prompt(window, cx);
            }
        });
        let passphrase_subscription =
            cx.subscribe_in(&passphrase, window, |this, _, event, window, cx| {
                if matches!(
                    event,
                    InputEvent::PressEnter {
                        secondary: false,
                        shift: false
                    }
                ) {
                    this.connect(window, cx);
                }
            });
        let (outbound, incoming) = async_channel::unbounded();
        let saved_connections = saved_connections();
        let selected_connection = saved_connections
            .first()
            .map(|connection| connection.id.clone());
        let protection = saved_connections
            .first()
            .map(|connection| Protection::from_code(connection.protection))
            .unwrap_or(Protection::None);
        let phone_unlock_available = canUsePhoneUnlock();
        Self {
            client_cancel: None,
            projects: Vec::new(),
            agent_options: Vec::new(),
            selected: None,
            pending_new_agent: None,
            new_session: false,
            new_project: None,
            creating_session: false,
            saved_connections,
            selected_connection,
            show_unlock: false,
            conversation_scroll: ScrollHandle::new(),
            composer,
            passphrase,
            outbound,
            incoming,
            pending_pairing: remote,
            protection,
            phone_unlock_available,
            connected: false,
            unlocking: false,
            scanning: false,
            generation: 0,
            connection_status: String::new(),
            action_error: None,
            prompt_queued: false,
            _subscriptions: vec![subscription, passphrase_subscription],
        }
    }

    pub(super) fn scan_qr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scanning || self.unlocking || self.connected {
            return;
        }
        self.scanning = true;
        self.action_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                startQrCamera().await?;
                loop {
                    let frame = nextQrFrame().await?;
                    if let Some(pairing) =
                        decode_qr_frame(&frame).map_err(|error| JsValue::from_str(&error))?
                    {
                        break Ok(pairing);
                    }
                }
            }
            .await;
            stopQrCamera();
            let _ = this.update_in(cx, |view, window, cx| {
                view.scanning = false;
                match result {
                    Ok(pairing) => {
                        view.pending_pairing = Some(pairing);
                        view.passphrase.update(cx, |input, cx| {
                            input.set_placeholder(
                                "Choose a passphrase (15+ characters)",
                                window,
                                cx,
                            )
                        });
                        view.show_unlock = false;
                        view.connection_status.clear();
                        view.action_error = None;
                    }
                    Err(error) => {
                        let message = js_error(error);
                        if message != "Camera scan cancelled" {
                            view.action_error = Some(message);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_saved_connection(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(connection) = self
            .saved_connections
            .iter()
            .find(|connection| connection.id == id)
        else {
            return;
        };
        self.protection = Protection::from_code(connection.protection);
        self.selected_connection = Some(id);
        self.pending_pairing = None;
        self.show_unlock = true;
        self.action_error = None;
        self.connection_status.clear();
        self.passphrase.update(cx, |input, cx| {
            input.set_placeholder("Unlock passphrase", window, cx)
        });
        cx.notify();
        if self.protection == Protection::Phone && self.phone_unlock_available {
            self.connect_with_phone(window, cx);
        }
    }

    fn show_saved_desktops(&mut self, cx: &mut Context<Self>) {
        self.pending_pairing = None;
        self.show_unlock = false;
        self.connection_status.clear();
        self.action_error = None;
        cx.notify();
    }

    fn rename_saved_connection(&mut self, id: String, cx: &mut Context<Self>) {
        if renameSavedConnection(id) {
            self.saved_connections = saved_connections();
            cx.notify();
        }
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.unlocking || self.connected {
            return;
        }
        let passphrase = self.passphrase.read(cx).value().to_string();
        if passphrase.is_empty() {
            self.action_error = Some("Enter your unlock passphrase".into());
            cx.notify();
            return;
        }
        if self.pending_pairing.is_some() && passphrase.chars().count() < 15 {
            self.action_error = Some("Use at least 15 characters for the unlock passphrase".into());
            cx.notify();
            return;
        }
        let selected_id = self.selected_connection.clone();
        if self.pending_pairing.is_none() && selected_id.is_none() {
            self.action_error = Some("Choose a saved desktop first".into());
            cx.notify();
            return;
        }
        let pending = self.pending_pairing.take();
        self.unlocking = true;
        self.connection_status = "Unlocking…".into();
        self.action_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = if let Some((remote, pairing_token)) = pending.as_ref() {
                let result = async {
                    let token = exchange_pairing(*remote, pairing_token).await?;
                    let id = saveConnection(passphrase, format!("{remote}:{token}"))
                        .await?
                        .as_string()
                        .ok_or_else(|| JsValue::from_str("Could not save connection"))?;
                    Ok((*remote, token, id))
                };
                result.await
            } else {
                let id = selected_id.unwrap();
                unlockConnection(passphrase, id.clone())
                    .await
                    .and_then(|value| {
                        value
                            .as_string()
                            .and_then(|value| parse_pairing(&value))
                            .map(|(endpoint, token)| (endpoint, token, id))
                            .ok_or_else(|| JsValue::from_str("Saved connection is damaged"))
                    })
            };
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok((endpoint, token, id)) => {
                    view.protection = Protection::Passphrase;
                    view.selected_connection = Some(id);
                    view.saved_connections = saved_connections();
                    view.finish_unlock(endpoint, token, window, cx);
                }
                Err(error) => {
                    view.unlocking = false;
                    view.pending_pairing = pending;
                    view.connection_status = "Locked".into();
                    view.action_error = Some(js_error(error));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn connect_with_phone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.unlocking || self.connected {
            return;
        }
        let selected_id = self.selected_connection.clone();
        if self.pending_pairing.is_none() && selected_id.is_none() {
            self.action_error = Some("Choose a saved desktop first".into());
            cx.notify();
            return;
        }
        let pending = self.pending_pairing.take();
        let ceremony = if pending.is_some() {
            beginPasskeyEnrollment()
        } else {
            beginPasskeyUnlock(selected_id.clone().unwrap())
        };
        self.unlocking = true;
        self.connection_status = "Waiting for phone unlock…".into();
        self.action_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let value = JsFuture::from(ceremony).await?;
                if let Some((remote, pairing_token)) = pending.as_ref() {
                    let token = exchange_pairing(*remote, pairing_token).await?;
                    let id = finishPasskeyEnrollment(format!("{remote}:{token}"))
                        .await?
                        .as_string()
                        .ok_or_else(|| JsValue::from_str("Could not save connection"))?;
                    Ok((*remote, token, id))
                } else {
                    let id = selected_id.unwrap();
                    value
                        .as_string()
                        .and_then(|value| parse_pairing(&value))
                        .map(|(endpoint, token)| (endpoint, token, id))
                        .ok_or_else(|| JsValue::from_str("Saved connection is damaged"))
                }
            }
            .await;
            discardPasskeyEnrollment();
            let _ = this.update_in(cx, |view, window, cx| match result {
                Ok((endpoint, token, id)) => {
                    view.protection = Protection::Phone;
                    view.selected_connection = Some(id);
                    view.saved_connections = saved_connections();
                    view.finish_unlock(endpoint, token, window, cx);
                }
                Err(error) => {
                    view.unlocking = false;
                    view.pending_pairing = pending;
                    view.connection_status = "Locked".into();
                    view.action_error = Some(js_error(error));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn finish_unlock(
        &mut self,
        endpoint: EndpointId,
        token: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.unlocking = false;
        self.passphrase
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.pending_pairing = None;
        self.connected = true;
        self.generation += 1;
        self.connection_status = "Connecting…".into();
        self.start_client(endpoint, token, self.generation, window, cx);
        cx.notify();
    }

    fn lock(&mut self, cx: &mut Context<Self>) {
        self.client_cancel.take();
        self.generation += 1;
        self.connected = false;
        self.unlocking = false;
        self.show_unlock = false;
        self.projects.clear();
        self.agent_options.clear();
        self.selected = None;
        self.pending_new_agent = None;
        self.new_session = false;
        self.new_project = None;
        self.creating_session = false;
        self.prompt_queued = false;
        while self.incoming.try_recv().is_ok() {}
        self.connection_status.clear();
        cx.notify();
    }

    fn start_client(
        &mut self,
        remote: EndpointId,
        token: String,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (events_tx, events_rx) = async_channel::unbounded();
        let (cancel_tx, cancel_rx) = async_channel::bounded(1);
        self.client_cancel = Some(cancel_tx);
        wasm_bindgen_futures::spawn_local(crate::connection::run(
            remote,
            token,
            self.incoming.clone(),
            events_tx,
            cancel_rx,
        ));
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events_rx.recv().await {
                if this
                    .update_in(cx, |view, window, cx| {
                        if view.generation != generation {
                            return;
                        }
                        match event {
                            crate::connection::ClientEvent::Hidden => view.lock(cx),
                            crate::connection::ClientEvent::Error(message) => {
                                view.connection_status = message;
                                cx.notify();
                            }
                            crate::connection::ClientEvent::Command {
                                prompt,
                                creating,
                                created,
                                error,
                            } => {
                                if view.generation != generation {
                                    return;
                                }
                                if let Some(prompt) = prompt {
                                    view.prompt_queued = false;
                                    if error.is_none()
                                        && view.composer.read(cx).value().as_ref() == prompt
                                    {
                                        view.composer.update(cx, |input, cx| {
                                            input.set_value("", window, cx)
                                        });
                                    }
                                }
                                if creating {
                                    view.creating_session = false;
                                    if let Some(agent_id) = created {
                                        view.selected = Some(agent_id);
                                        view.pending_new_agent = Some(agent_id);
                                        view.new_session = false;
                                        view.new_project = None;
                                        view.conversation_scroll.scroll_to_bottom();
                                    }
                                }
                                view.action_error = error;
                                cx.notify();
                            }
                            crate::connection::ClientEvent::Snapshot {
                                projects,
                                agent_options,
                            } => {
                                let previous_selected = view.selected;
                                let previous_message = view.selected_agent().and_then(|agent| {
                                    agent
                                        .messages
                                        .last()
                                        .map(|message| (agent.messages.len(), message.text.len()))
                                });
                                let near_bottom = view.conversation_scroll.max_offset().y
                                    + view.conversation_scroll.offset().y
                                    < px(80.);
                                view.projects = projects;
                                view.agent_options = agent_options;
                                if view.pending_new_agent.is_some_and(|id| {
                                    view.projects.iter().any(|project| {
                                        project.agents.iter().any(|agent| agent.id == id)
                                    })
                                }) {
                                    view.pending_new_agent = None;
                                }
                                if view.pending_new_agent.is_none()
                                    && view.selected.is_none_or(|id| {
                                        !view.projects.iter().any(|project| {
                                            project.agents.iter().any(|agent| agent.id == id)
                                        })
                                    })
                                {
                                    view.selected = view
                                        .projects
                                        .iter()
                                        .flat_map(|project| &project.agents)
                                        .next()
                                        .map(|agent| agent.id);
                                }
                                let current_message = view.selected_agent().and_then(|agent| {
                                    agent
                                        .messages
                                        .last()
                                        .map(|message| (agent.messages.len(), message.text.len()))
                                });
                                if previous_selected != view.selected
                                    || (near_bottom && previous_message != current_message)
                                {
                                    view.conversation_scroll.scroll_to_bottom();
                                }
                                view.connection_status = "Connected".into();
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn open_new_session(&mut self, cx: &mut Context<Self>) {
        self.new_session = true;
        self.new_project = None;
        self.action_error = None;
        cx.notify();
    }

    fn back_from_new_session(&mut self, cx: &mut Context<Self>) {
        if self.creating_session {
            return;
        }
        if self.new_project.is_some() {
            self.new_project = None;
        } else {
            self.new_session = false;
        }
        self.action_error = None;
        cx.notify();
    }

    fn choose_new_project(&mut self, project: String, cx: &mut Context<Self>) {
        self.new_project = Some(project);
        self.action_error = None;
        cx.notify();
    }

    fn prompt_new_project(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = promptProjectPath().as_string() {
            self.choose_new_project(project, cx);
        }
    }

    fn start_new_session(
        &mut self,
        command: Vec<String>,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.creating_session {
            return;
        }
        let Some(project) = self.new_project.clone() else {
            return;
        };
        if self
            .outbound
            .try_send(Command::NewSession {
                project,
                command,
                name,
            })
            .is_ok()
        {
            self.creating_session = true;
            self.action_error = None;
        } else {
            self.action_error = Some("Could not start a new session".into());
        }
        cx.notify();
    }

    fn start_custom_session(&mut self, cx: &mut Context<Self>) {
        let Some(value) = promptAcpCommand().as_string() else {
            return;
        };
        match shell_words::split(&value) {
            Ok(command) if !command.is_empty() => self.start_new_session(command, None, cx),
            _ => {
                self.action_error = Some("Enter an ACP executable and its arguments".into());
                cx.notify();
            }
        }
    }

    fn send_prompt(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.prompt_queued {
            return;
        }
        let Some(agent_id) = self.selected else {
            return;
        };
        let text = self.composer.read(cx).value().to_string();
        let text = text.strip_suffix('\n').unwrap_or(&text).to_owned();
        if text.trim().is_empty() {
            return;
        }
        if self
            .outbound
            .try_send(Command::Prompt { agent_id, text })
            .is_ok()
        {
            self.prompt_queued = true;
            self.action_error = None;
            cx.notify();
        } else {
            self.action_error = Some("Could not queue prompt".into());
            cx.notify();
        }
    }

    fn selected_agent(&self) -> Option<&Agent> {
        let id = self.selected?;
        self.projects
            .iter()
            .flat_map(|project| &project.agents)
            .find(|agent| agent.id == id)
    }
}
