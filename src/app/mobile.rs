use super::*;
use agentaps_control_protocol::{
    Agent as RemoteAgent, AgentOption, Command as RemoteCommand, Message as RemoteMessage,
    Permission as RemotePermission, PermissionOption, Project as RemoteProject, Response,
};
use gpui_kit::ClipboardItem;
use qrcode::{Color, QrCode};

fn mobile_text(text: &str) -> String {
    let Some((end, _)) = text.char_indices().nth(4_000) else {
        return text.to_owned();
    };
    format!("{}\n[Message shortened on mobile]", &text[..end])
}

impl Workspace {
    pub(super) fn mobile_link(&self) -> Option<String> {
        self.mobile_access.link(self.web_connect_url.as_deref())
    }

    pub(super) fn show_mobile_link(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mobile_access.server.is_none() {
            self.mobile_access.pairing_visible = true;
            if !self.mobile_access.loading {
                self.start_mobile(None);
            }
        } else if let Some(link) = self.mobile_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
            self.mobile_access.qr = QrCode::new(link.as_bytes()).ok().map(|qr| {
                qr.to_colors()
                    .chunks(qr.width())
                    .map(|row| row.iter().map(|color| *color == Color::Dark).collect())
                    .collect()
            });
            self.mobile_access.pairing_visible = !self.mobile_access.pairing_visible;
            self.notice = Some(Notice::Info(
                if self.mobile_access.pairing_visible {
                    "Mobile link copied. Scan the code or open the copied link on your phone."
                } else {
                    "Mobile pairing code hidden. Mobile access is still on."
                }
                .into(),
            ));
        } else {
            self.mobile_access.pairing_visible = !self.mobile_access.pairing_visible;
            self.notice = Some(Notice::Info("Mobile access is starting".into()));
        }
        cx.notify();
    }

    fn start_mobile(&mut self, provider: Option<String>) {
        self.mobile_access.loading = true;
        self.mobile_access.provider_prompt = None;
        self.notice = None;
        let sender = self.mobile_access.start_tx.clone();
        std::thread::spawn(move || {
            let result = match provider {
                Some(provider) => crate::mobile::start_with_provider(Some(&provider)),
                None => crate::mobile::start(),
            };
            let _ = sender.send(result);
        });
    }

    pub(super) fn use_mobile_provider(&mut self, cx: &mut Context<Self>) {
        let provider = self
            .mobile_access
            .provider_input
            .read(cx)
            .value()
            .trim()
            .to_owned();
        self.use_mobile_provider_named(&provider, cx);
    }

    pub(super) fn use_mobile_provider_named(&mut self, provider: &str, cx: &mut Context<Self>) {
        if provider.is_empty() {
            self.mobile_access.provider_prompt =
                Some("Enter a SecretSpec provider name or URI.".into());
            cx.notify();
            return;
        }
        self.mobile_access.pairing_visible = true;
        self.start_mobile(Some(provider.to_owned()));
        cx.notify();
    }

    pub(super) fn copy_mobile_link(&mut self, cx: &mut Context<Self>) {
        if let Some(link) = self.mobile_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(link));
            self.notice = Some(Notice::Info(
                "Mobile link copied. Open it on your phone to pair.".into(),
            ));
            cx.notify();
        }
    }

    pub(super) fn revoke_mobile_client(&mut self, id: String, cx: &mut Context<Self>) {
        if self.mobile_access.revoke_confirm.as_deref() != Some(&id) {
            self.mobile_access.revoke_confirm = Some(id);
            cx.notify();
            return;
        }
        self.mobile_access.revoke_confirm = None;
        if let Some(server) = &self.mobile_access.server {
            server.revoke_client(id);
            self.notice = Some(Notice::Info("Revoking linked client…".into()));
        }
        cx.notify();
    }

    pub(super) fn poll_mobile(&mut self, cx: &mut Context<Self>) {
        if let Ok(result) = self.mobile_access.start_rx.try_recv() {
            self.mobile_access.loading = false;
            match result {
                Ok(server) => {
                    self.mobile_access.server = Some(server);
                    self.notice = Some(Notice::Info("Connecting mobile access…".into()));
                }
                Err(error) => {
                    self.mobile_access.pairing_visible = false;
                    self.mobile_access.provider_prompt = Some(error);
                }
            }
            cx.notify();
        }
        let Some(server) = &self.mobile_access.server else {
            return;
        };
        let statuses: Vec<_> = server.status.try_iter().collect();
        let revocations: Vec<_> = server.revoke_results.try_iter().collect();
        let commands: Vec<_> = server.commands.try_iter().collect();
        let handled_command = !commands.is_empty();
        let received_status = !statuses.is_empty();
        let mut startup_error = None;
        for status in statuses {
            match status {
                Ok(endpoint_id) => {
                    let already_ready = self.mobile_access.endpoint_id.is_some();
                    self.mobile_access.endpoint_id = Some(endpoint_id);
                    if already_ready {
                        self.mobile_access.qr = self.mobile_link().and_then(|link| {
                            QrCode::new(link.as_bytes()).ok().map(|qr| {
                                qr.to_colors()
                                    .chunks(qr.width())
                                    .map(|row| {
                                        row.iter().map(|color| *color == Color::Dark).collect()
                                    })
                                    .collect()
                            })
                        });
                        self.notice = Some(Notice::Info(
                            "Browser linked. A fresh pairing code is ready.".into(),
                        ));
                    } else if let Some(link) = self.mobile_link() {
                        self.mobile_access.qr = QrCode::new(link.as_bytes()).ok().map(|qr| {
                            qr.to_colors()
                                .chunks(qr.width())
                                .map(|row| row.iter().map(|color| *color == Color::Dark).collect())
                                .collect()
                        });
                        cx.write_to_clipboard(ClipboardItem::new_string(link));
                        self.notice = Some(Notice::Info("Mobile link copied. Scan the code or open the copied link on your phone.".into()));
                    }
                }
                Err(error) => startup_error = Some(error),
            }
            cx.notify();
        }
        if let Some(error) = startup_error {
            self.mobile_access.server = None;
            self.mobile_access.endpoint_id = None;
            self.mobile_access.pairing_visible = false;
            self.notice = Some(Notice::Error(format!("Mobile access failed: {error}")));
            cx.notify();
            return;
        }
        for result in revocations {
            self.notice = Some(match result {
                Ok(_) => Notice::Info("Linked client revoked.".into()),
                Err(error) => Notice::Error(format!("Could not revoke linked client: {error}")),
            });
            cx.notify();
        }
        for pending in commands {
            let response = match self.apply_mobile_command(pending.command, cx) {
                Ok(response) => response,
                Err(message) => Response::Error { message },
            };
            let _ = pending.reply.send(response);
        }
        if handled_command
            || received_status
            || self
                .mobile_access
                .last_snapshot
                .is_none_or(|last| last.elapsed() >= Duration::from_millis(500))
        {
            let snapshot = self.mobile_snapshot();
            if let Some(server) = &self.mobile_access.server {
                *server.snapshot.lock().unwrap() = snapshot;
            }
            self.mobile_access.last_snapshot = Some(Instant::now());
        }
    }

    fn mobile_snapshot(&self) -> Response {
        let mut agent_options: Vec<AgentOption> = self
            .picker
            .available_agents
            .iter()
            .map(|agent| AgentOption {
                name: agent.name.clone(),
                command: agent.command.clone(),
            })
            .collect();
        for agent in self.projects.iter().flat_map(|project| &project.agents) {
            if !agent_options
                .iter()
                .any(|option| option.command == agent.config.command)
            {
                agent_options.push(AgentOption {
                    name: agent.name.clone(),
                    command: agent.config.command.clone(),
                });
            }
        }
        Response::Snapshot {
            agent_options,
            projects: self
                .projects
                .iter()
                .map(|project| RemoteProject {
                    path: project.display_path(),
                    agents: project
                        .agents
                        .iter()
                        .filter(|agent| !agent.config.archived)
                        .map(|agent| RemoteAgent {
                            id: agent.config.id,
                            name: agent.name.clone(),
                            status: agent.status.label().into(),
                            active: agent.active_work,
                            has_older_messages: agent.messages.len() > 100,
                            messages: agent
                                .messages
                                .iter()
                                .skip(agent.messages.len().saturating_sub(100))
                                .map(|message| RemoteMessage {
                                    role: format!("{:?}", message.role).to_lowercase(),
                                    text: mobile_text(&message.transcript_text()),
                                })
                                .collect(),
                            permissions: agent
                                .permissions
                                .iter()
                                .map(|permission| RemotePermission {
                                    request_id: permission.request_id.to_string(),
                                    title: permission.title.clone(),
                                    description: permission.description.clone(),
                                    options: permission
                                        .options
                                        .iter()
                                        .map(|(id, label)| PermissionOption {
                                            id: id.clone(),
                                            label: label.clone(),
                                        })
                                        .collect(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn mobile_agent_mut(&mut self, agent_id: u64) -> Option<&mut AgentView> {
        self.projects
            .iter_mut()
            .flat_map(|project| &mut project.agents)
            .find(|agent| agent.config.id == agent_id && !agent.config.archived)
    }

    fn apply_mobile_command(
        &mut self,
        command: RemoteCommand,
        cx: &mut Context<Self>,
    ) -> Result<Response, String> {
        match command {
            RemoteCommand::Pair | RemoteCommand::Snapshot => {}
            RemoteCommand::NewSession {
                project,
                command,
                name,
            } => {
                if project.is_empty() || project.len() > 4096 {
                    return Err("Enter a project path".into());
                }
                if command.is_empty()
                    || command.len() > 32
                    || command.iter().any(|arg| arg.is_empty() || arg.len() > 1024)
                {
                    return Err("Enter a valid ACP command".into());
                }
                if name.as_ref().is_some_and(|name| name.len() > 128) {
                    return Err("Agent name is too long".into());
                }
                let project_index = if let Some(index) = self
                    .projects
                    .iter()
                    .position(|existing| existing.display_path() == project)
                {
                    index
                } else {
                    let (path, ssh_host) = match crate::remote::parse_project(&project)? {
                        Some(remote) => (remote.path, Some(remote.host)),
                        None => {
                            let path = PathBuf::from(&project);
                            if !path.is_absolute() {
                                return Err("Enter an absolute project path".into());
                            }
                            let path = path
                                .canonicalize()
                                .map_err(|_| "Project folder does not exist".to_string())?;
                            if !path.is_dir() {
                                return Err("Project path is not a folder".into());
                            }
                            (path, None)
                        }
                    };
                    if let Some(index) = self
                        .projects
                        .iter()
                        .position(|existing| existing.path == path && existing.ssh_host == ssh_host)
                    {
                        index
                    } else {
                        self.projects.push(ProjectView {
                            sync_counts: None,
                            branch: ssh_host.clone().unwrap_or_else(|| branch(&path)),
                            path: path.clone(),
                            ssh_host: ssh_host.clone(),
                            agents: Vec::new(),
                        });
                        self.picker.folder_search.add_recent(match &ssh_host {
                            Some(host) => PathBuf::from(crate::remote::project_label(host, &path)),
                            None => path,
                        });
                        self.projects.len() - 1
                    }
                };
                let (agent_id, _) = self.create_agent_for_project(project_index, command, name, cx);
                return Ok(Response::SessionCreated { agent_id });
            }
            RemoteCommand::Prompt { agent_id, text } => {
                if text.trim().is_empty() {
                    return Err("Prompt is empty".into());
                }
                let Some(agent) = self.mobile_agent_mut(agent_id) else {
                    return Err("Session not found".into());
                };
                if agent.session_id.is_none() {
                    return Err("Agent is still connecting".into());
                }
                if agent.active_work || !agent.config.pending_prompts.is_empty() {
                    agent.config.pending_prompts.push(text.clone().into());
                } else if let Err(error) = agent.start_prompt(text.clone().into()) {
                    agent.status = Status::Error;
                    agent.log(Role::System, error);
                    cx.notify();
                    return Err("Could not send prompt".into());
                }
                agent.config.prompt_history.push(text);
                self.persistence.dirty = true;
                cx.notify();
            }
            RemoteCommand::Cancel { agent_id } => {
                let Some(agent) = self.mobile_agent_mut(agent_id) else {
                    return Err("Session not found".into());
                };
                if !agent.active_work || agent.cancel_requested {
                    return Err("Agent is not working".into());
                }
                let Some(session_id) = &agent.controller.session_id else {
                    return Err("Agent is still connecting".into());
                };
                match agent.controller.send(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session_id}})) {
                    Ok(()) => agent.cancel_requested = true,
                    Err(error) => {
                        agent.log(Role::System, format!("Could not stop agent: {error}"));
                        return Err("Could not stop agent".into());
                    }
                }
                cx.notify();
            }
            RemoteCommand::Permission {
                agent_id,
                request_id,
                option_id,
            } => {
                let Some(agent) = self.mobile_agent_mut(agent_id) else {
                    return Err("Session not found".into());
                };
                let request_id: Value = serde_json::from_str(&request_id)
                    .map_err(|_| "Invalid permission request ID".to_owned())?;
                let Some(index) = agent.permissions.iter().position(|permission| {
                    permission.request_id == request_id
                        && permission.options.iter().any(|(id, _)| *id == option_id)
                }) else {
                    return Err("Permission request is no longer pending".into());
                };
                let permission = agent.permissions.remove(index);
                if let Err(error) =
                    agent.send(json!({"jsonrpc":"2.0","id":permission.request_id,"result":{
                        "outcome":{"outcome":"selected","optionId":option_id}
                    }}))
                {
                    agent.status = Status::Error;
                    agent.log(Role::System, error);
                    return Err("Could not answer permission request".into());
                }
                cx.notify();
            }
        }
        Ok(Response::Accepted)
    }
}

#[cfg(test)]
pub(super) fn verify_session_switching_and_mobile_routing(
    workspace: &Entity<Workspace>,
    root: &Path,
    cx: &mut gpui_kit::VisualTestContext,
) {
    use crate::session::test_agent;
    use agentaps_control_protocol::{Command, Response};

    let first = SessionLocation {
        project_index: 0,
        agent_index: 0,
    };
    let second = SessionLocation {
        project_index: 0,
        agent_index: 1,
    };
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            let mut first_agent = test_agent(ProtocolVersion::V1);
            first_agent.config.id = 101;
            let mut second_agent = test_agent(ProtocolVersion::V2);
            second_agent.config.id = 102;
            this.projects = vec![ProjectView {
                path: root.to_owned(),
                ssh_host: None,
                branch: String::new(),
                sync_counts: None,
                agents: vec![
                    AgentView {
                        controller: first_agent,
                        elicitations: Vec::new(),
                    },
                    AgentView {
                        controller: second_agent,
                        elicitations: Vec::new(),
                    },
                ],
            }];
            this.set_view(WorkspaceView::Conversation(first), window, cx);
            this.conversation.composer.update(cx, |input, cx| {
                input.set_value("First draft", window, cx);
                input.set_cursor_position(Position::new(0, 3), window, cx);
            });
            let first_composer = this.conversation.composer.clone();
            this.conversation.prompt_recall = Some(PromptRecall {
                agent_id: 101,
                index: 0,
                draft: "First draft".into(),
                displayed: "Earlier".into(),
            });
            this.set_view(WorkspaceView::Conversation(second), window, cx);
            assert_ne!(
                first_composer.entity_id(),
                this.conversation.composer.entity_id()
            );
            assert!(this.conversation.composer.read(cx).value().is_empty());
            assert!(this.conversation.prompt_recall.is_none());
            this.conversation
                .composer
                .update(cx, |input, cx| input.set_value("Second draft", window, cx));
            let second_composer = this.conversation.composer.clone();
            this.set_view(WorkspaceView::Conversation(first), window, cx);
            assert_eq!(
                this.conversation.composer.entity_id(),
                first_composer.entity_id()
            );
            assert_eq!(
                this.conversation.composer.read(cx).value().as_ref(),
                "First draft"
            );
            assert_eq!(
                this.conversation.composer.read(cx).cursor_position(),
                Position::new(0, 3)
            );
            this.set_view(this.view.toggle_archive(), window, cx);
            this.set_view(this.view.toggle_archive(), window, cx);
            assert_eq!(
                this.conversation.composer.entity_id(),
                first_composer.entity_id()
            );

            // Web commands address their session ID even while another session is displayed.
            let response = this
                .apply_mobile_command(
                    Command::Prompt {
                        agent_id: 102,
                        text: "Remote request".into(),
                    },
                    cx,
                )
                .unwrap();
            assert!(matches!(response, Response::Accepted));
            assert!(this.projects[0].agents[0].config.pending_prompts.is_empty());
            assert_eq!(
                this.projects[0].agents[1].config.pending_prompts,
                [Prompt::from("Remote request")]
            );
            assert_eq!(second_composer.read(cx).value().as_ref(), "Second draft");
            assert_eq!(this.view, WorkspaceView::Conversation(first));
            assert!(
                this.apply_mobile_command(
                    Command::Prompt {
                        agent_id: 999,
                        text: "Missing".into()
                    },
                    cx
                )
                .is_err()
            );
            assert!(
                this.apply_mobile_command(
                    Command::Prompt {
                        agent_id: 102,
                        text: " ".into()
                    },
                    cx
                )
                .is_err()
            );
            this.projects[0].agents[1].config.archived = true;
            assert!(
                this.apply_mobile_command(
                    Command::Prompt {
                        agent_id: 102,
                        text: "Archived".into()
                    },
                    cx
                )
                .is_err()
            );
            let Response::Snapshot { projects, .. } = this.mobile_snapshot() else {
                panic!("expected snapshot")
            };
            assert_eq!(projects[0].agents.len(), 1);
            assert_eq!(projects[0].agents[0].id, 101);
            // Keep these disconnected fixtures out of the zoom test's saved configuration.
            this.projects.clear();
            this.view = WorkspaceView::Empty;
            this.persistence.dirty = false;
        })
    });
}
