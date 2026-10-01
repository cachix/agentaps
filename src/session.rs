//! Session behavior independent of GPUI entities and rendering.
use crate::acp::Connection;
use crate::config::{
    AgentConfig, ChatEntry, ChatImage, ForkSource, Prompt, Role, SlashCommand,
    with_image_placeholders,
};
use crate::images::ImageStore;
use agent_client_protocol_schema::{ProtocolVersion, v2};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};

mod prompt;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::agent as test_agent;
mod protocol;
use prompt::fork_prompt;
pub(crate) use prompt::prompt_for_agent;
pub(crate) use prompt::{shell_command, shell_command_in_message};
pub(crate) use protocol::initialize_params;
use protocol::{ImageData, SessionUpdate, accepts_images, decode_update};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Connecting,
    Idle,
    Working,
    Done,
    Error,
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestoreMode {
    Resume,
    Load,
}

pub(crate) fn restore_mode(protocol: ProtocolVersion, initialize: &Value) -> Option<RestoreMode> {
    match protocol {
        ProtocolVersion::V2 => Some(RestoreMode::Resume),
        ProtocolVersion::V1 => {
            let capabilities = &initialize["agentCapabilities"];
            if capabilities["loadSession"].as_bool() == Some(true) {
                Some(RestoreMode::Load)
            } else if capabilities["sessionCapabilities"]["resume"].is_object() {
                Some(RestoreMode::Resume)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) struct Permission {
    pub(crate) request_id: Value,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) options: Vec<(String, String)>,
}

pub(crate) struct OAuthRetry {
    pub(crate) request: Value,
    pub(crate) due: Option<Instant>,
    pub(crate) attempted: bool,
    pub(crate) accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigChoice {
    pub(crate) value: String,
    pub(crate) label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConfigSelectOption {
    pub(crate) id: String,
    pub(crate) current: String,
    pub(crate) choices: Vec<ConfigChoice>,
}

impl ConfigSelectOption {
    pub(crate) fn label(&self) -> String {
        self.choices
            .iter()
            .find(|choice| choice.value == self.current)
            .map(|choice| choice.label.clone())
            .unwrap_or_else(|| self.current.clone())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigOptionKind {
    Model,
    Effort,
    Mode,
    Collaboration,
}

/// A two-state Plan control for agents, such as Codex, that report planning
/// as a collaboration mode separate from their permission modes.
pub(crate) struct PlanToggle {
    pub(crate) active: bool,
    /// The value that selecting the toggle sends.
    pub(crate) next: String,
}

pub(crate) struct SessionController {
    pub(crate) config: AgentConfig,
    pub(crate) name: String,
    pub(crate) model: Option<String>,
    pub(crate) model_option: Option<ConfigSelectOption>,
    pub(crate) pending_model: Option<(u64, String)>,
    pub(crate) effort_option: Option<ConfigSelectOption>,
    pub(crate) pending_effort: Option<(u64, String)>,
    pub(crate) mode_option: Option<ConfigSelectOption>,
    /// The modes came from the session's `modes` field rather than a config
    /// option, so they change through `session/set_mode`.
    pub(crate) legacy_modes: bool,
    pub(crate) pending_mode: Option<(u64, String)>,
    pub(crate) collaboration_option: Option<ConfigSelectOption>,
    pub(crate) pending_collaboration: Option<(u64, String)>,
    pub(crate) context: Option<(u64, u64)>,
    pub(crate) status: Status,
    pub(crate) protocol: Option<ProtocolVersion>,
    pub(crate) accepts_images: bool,
    pub(crate) images: ImageStore,
    pub(crate) active_work: bool,
    pub(crate) awaiting_response: bool,
    pub(crate) cancel_requested: bool,
    pub(crate) session_id: Option<String>,
    pub(crate) restoring: Option<RestoreMode>,
    pub(crate) recovery_due: Option<Instant>,
    pub(crate) oauth_retry: Option<OAuthRetry>,
    pub(crate) next_request_id: u64,
    pub(crate) messages: Vec<ChatEntry>,
    pub(crate) permissions: Vec<Permission>,
    pub(crate) connection: Option<Connection>,
}

pub(crate) fn agent_name(command: &[String]) -> String {
    command
        .first()
        .and_then(|name| Path::new(name).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Agent".into())
}

pub(crate) fn model_option(config_options: &Value) -> Option<ConfigSelectOption> {
    let option = config_options.as_array()?.iter().find(|option| {
        option["category"].as_str() == Some("model")
            || option["configId"].as_str() == Some("model")
            || option["id"].as_str() == Some("model")
    })?;
    parse_config_select_option(option)
}

pub(crate) fn effort_option(config_options: &Value) -> Option<ConfigSelectOption> {
    let option = config_options.as_array()?.iter().find(|option| {
        let id = option["configId"]
            .as_str()
            .or_else(|| option["id"].as_str())
            .unwrap_or_default();
        option["category"].as_str() == Some("thought_level")
            || id.contains("effort")
            || (option["category"].as_str() == Some("model_config")
                && option["name"]
                    .as_str()
                    .is_some_and(|name| name.to_lowercase().contains("reasoning")))
    })?;
    parse_config_select_option(option)
}

pub(crate) fn mode_option(config_options: &Value) -> Option<ConfigSelectOption> {
    let option = config_options.as_array()?.iter().find(|option| {
        option["category"].as_str() == Some("mode")
            || option["configId"].as_str() == Some("mode")
            || option["id"].as_str() == Some("mode")
    })?;
    parse_config_select_option(option)
}

pub(crate) fn collaboration_option(config_options: &Value) -> Option<ConfigSelectOption> {
    let option = config_options.as_array()?.iter().find(|option| {
        option["category"].as_str() == Some("collaboration_mode")
            || option["configId"].as_str() == Some("collaboration_mode")
            || option["id"].as_str() == Some("collaboration_mode")
    })?;
    parse_config_select_option(option)
}

/// Reads the session's `modes` field, which agents without a mode config
/// option use to report their modes. A mode config option takes precedence.
pub(crate) fn update_session_modes(agent: &mut SessionController, modes: &Value) {
    if agent.mode_option.is_some() && !agent.legacy_modes {
        return;
    }
    let (Some(current), Some(available)) = (
        modes["currentModeId"].as_str(),
        modes["availableModes"].as_array(),
    ) else {
        return;
    };
    let choices = available
        .iter()
        .filter_map(|mode| {
            let value = mode["id"].as_str()?;
            Some(ConfigChoice {
                value: value.to_owned(),
                label: mode["name"].as_str().unwrap_or(value).to_owned(),
            })
        })
        .collect();
    agent.mode_option = Some(ConfigSelectOption {
        id: "mode".into(),
        current: current.to_owned(),
        choices,
    });
    agent.legacy_modes = true;
}

pub(crate) fn mode_request(
    id: u64,
    session_id: &str,
    option: &ConfigSelectOption,
    legacy: bool,
    value: &str,
) -> Value {
    if legacy {
        json!({"jsonrpc":"2.0","id":id,"method":"session/set_mode","params":{
            "sessionId":session_id,"modeId":value
        }})
    } else {
        set_config_option_request(id, session_id, option, value)
    }
}

fn parse_config_select_option(option: &Value) -> Option<ConfigSelectOption> {
    let id = option["configId"]
        .as_str()
        .or_else(|| option["id"].as_str())?;
    let current = option["currentValue"].as_str()?;
    let options = option["options"].as_array()?;
    let choices = options
        .iter()
        .flat_map(|entry| {
            entry["options"]
                .as_array()
                .map_or_else(|| vec![entry], |group| group.iter().collect())
        })
        .filter_map(|choice| {
            let value = choice["value"].as_str()?;
            let label = if value == "default" && option["category"].as_str() == Some("model") {
                choice["description"]
                    .as_str()
                    .and_then(|description| description.split(" · ").next())
                    .or_else(|| choice["name"].as_str())
            } else {
                choice["name"].as_str()
            }
            .unwrap_or(value);
            Some(ConfigChoice {
                value: value.to_owned(),
                label: label.to_owned(),
            })
        })
        .collect();
    Some(ConfigSelectOption {
        id: id.to_owned(),
        current: current.to_owned(),
        choices,
    })
}

pub(crate) fn set_config_option_request(
    id: u64,
    session_id: &str,
    option: &ConfigSelectOption,
    value: &str,
) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"session/set_config_option","params":{
        "sessionId":session_id,"configId":option.id,"type":"id","value":value
    }})
}

impl SessionController {
    /// A setting change is waiting for the agent's reply.
    pub(crate) fn setting_pending(&self) -> bool {
        self.pending_model.is_some()
            || self.pending_effort.is_some()
            || self.pending_mode.is_some()
            || self.pending_collaboration.is_some()
    }

    pub(crate) fn clear_pending_settings(&mut self) {
        self.pending_model = None;
        self.pending_effort = None;
        self.pending_mode = None;
        self.pending_collaboration = None;
    }

    pub(crate) fn setting_option(
        &mut self,
        kind: ConfigOptionKind,
    ) -> &mut Option<ConfigSelectOption> {
        match kind {
            ConfigOptionKind::Model => &mut self.model_option,
            ConfigOptionKind::Effort => &mut self.effort_option,
            ConfigOptionKind::Mode => &mut self.mode_option,
            ConfigOptionKind::Collaboration => &mut self.collaboration_option,
        }
    }

    pub(crate) fn pending_setting(&mut self, kind: ConfigOptionKind) -> &mut Option<(u64, String)> {
        match kind {
            ConfigOptionKind::Model => &mut self.pending_model,
            ConfigOptionKind::Effort => &mut self.pending_effort,
            ConfigOptionKind::Mode => &mut self.pending_mode,
            ConfigOptionKind::Collaboration => &mut self.pending_collaboration,
        }
    }

    /// The Plan toggle, when the collaboration mode offers `plan` and another
    /// value to return to.
    pub(crate) fn plan_toggle(&self) -> Option<PlanToggle> {
        let option = self.collaboration_option.as_ref()?;
        let plan = option
            .choices
            .iter()
            .find(|choice| choice.value == "plan")?;
        let other = option
            .choices
            .iter()
            .find(|choice| choice.value == "default")
            .or_else(|| option.choices.iter().find(|choice| choice.value != "plan"))?;
        let active = option.current == plan.value;
        Some(PlanToggle {
            active,
            next: if active { &other.value } else { &plan.value }.clone(),
        })
    }

    pub(crate) fn reset_config(&self, id: u64, mut messages: Vec<ChatEntry>) -> AgentConfig {
        for entry in &mut messages {
            entry.key = None;
        }
        AgentConfig {
            id,
            command: self.config.command.clone(),
            archived: self.config.archived,
            display_name: self.config.display_name.clone(),
            title: None,
            session_id: None,
            model: None,
            context: None,
            messages,
            available_commands: Vec::new(),
            pending_prompts: Vec::new(),
            active_prompt: None,
            prompt_history: Vec::new(),
            was_working: false,
            session_has_activity: false,
            fork_pending: false,
            fork_source: None,
        }
    }

    pub(crate) fn fork_config(&self, id: u64, response_index: usize) -> Option<AgentConfig> {
        if self.messages.get(response_index)?.role != Role::Agent {
            return None;
        }
        let mut messages = self.messages[..=response_index].to_vec();
        for entry in &mut messages {
            entry.key = None;
        }
        Some(AgentConfig {
            id,
            command: self.config.command.clone(),
            archived: false,
            display_name: self.config.display_name.clone(),
            title: None,
            session_id: None,
            model: None,
            context: None,
            prompt_history: messages
                .iter()
                .filter(|entry| entry.role == Role::User)
                .map(|entry| entry.text.clone())
                .collect(),
            messages,
            available_commands: Vec::new(),
            pending_prompts: Vec::new(),
            active_prompt: None,
            was_working: false,
            session_has_activity: false,
            fork_pending: true,
            fork_source: self
                .session_id
                .clone()
                .or_else(|| self.config.session_id.clone())
                .map(|session_id| ForkSource {
                    session_id,
                    reply_key: self.messages[response_index].key.clone(),
                    reply_index: self.messages[..=response_index]
                        .iter()
                        .filter(|entry| entry.role == Role::Agent)
                        .count()
                        - 1,
                }),
        })
    }

    pub(crate) fn mark_viewed(&mut self) {
        if self.status == Status::Done {
            self.status = Status::Idle;
        }
    }

    pub(crate) fn new(mut config: AgentConfig, images: ImageStore) -> Self {
        if config.prompt_history.is_empty() {
            config.prompt_history = config
                .messages
                .iter()
                .filter(|entry| entry.role == Role::User)
                .map(|entry| entry.text.clone())
                .chain(
                    config
                        .pending_prompts
                        .iter()
                        .map(|prompt| prompt.text.clone()),
                )
                .collect();
        }
        if config.was_working && config.active_prompt.is_none() {
            config.active_prompt = config
                .prompt_history
                .len()
                .checked_sub(config.pending_prompts.len() + 1)
                .and_then(|index| config.prompt_history.get(index).cloned());
        }
        let name = config
            .display_name
            .clone()
            .unwrap_or_else(|| agent_name(&config.command));
        let mut messages = std::mem::take(&mut config.messages);
        if config.was_working
            && !messages.last().is_some_and(|entry| {
                entry.role == Role::System && entry.text == "App closed while this turn was active."
            })
        {
            messages.push(ChatEntry {
                role: Role::System,
                key: None,
                text: "App closed while this turn was active.".into(),
                images: Vec::new(),
            });
        }
        let model = config.model.clone();
        let context = config.context;
        Self {
            config,
            name,
            model,
            model_option: None,
            pending_model: None,
            effort_option: None,
            pending_effort: None,
            mode_option: None,
            legacy_modes: false,
            pending_mode: None,
            collaboration_option: None,
            pending_collaboration: None,
            context,
            status: Status::Connecting,
            protocol: None,
            accepts_images: false,
            images,
            active_work: false,
            awaiting_response: false,
            cancel_requested: false,
            session_id: None,
            restoring: None,
            recovery_due: None,
            oauth_retry: None,
            next_request_id: 3,
            messages,
            permissions: Vec::new(),
            connection: None,
        }
    }

    pub(crate) fn snapshot(&self) -> AgentConfig {
        AgentConfig {
            id: self.config.id,
            command: self.config.command.clone(),
            archived: self.config.archived,
            display_name: self.config.display_name.clone(),
            title: self.config.title.clone(),
            session_id: self
                .session_id
                .clone()
                .or_else(|| self.config.session_id.clone()),
            model: self.model.clone(),
            context: self.context,
            messages: Vec::new(),
            available_commands: self.config.available_commands.clone(),
            pending_prompts: self.config.pending_prompts.clone(),
            active_prompt: self.config.active_prompt.clone(),
            prompt_history: Vec::new(),
            was_working: self.active_work || self.config.was_working,
            session_has_activity: self.config.session_has_activity || self.active_work,
            fork_pending: self.config.fork_pending,
            fork_source: self.config.fork_source.clone(),
        }
    }

    pub(crate) fn has_restorable_activity(&self) -> bool {
        self.config.session_has_activity || self.config.was_working
    }

    pub(crate) fn continuation_prompt(&self) -> String {
        let mut prompt = String::from(
            "Continue the task interrupted when Agentaps closed. Inspect the current state and avoid repeating completed actions.",
        );
        if let Some(previous) = &self.config.active_prompt {
            prompt.push_str("\n\nThe interrupted request was:\n\n");
            prompt.push_str(previous);
        }
        prompt
    }

    pub(crate) fn auto_continue_interrupted_turn(&mut self) -> bool {
        if !self.config.was_working
            || self.active_work
            || self.restoring.is_some()
            || !matches!(self.status, Status::Idle | Status::Done)
            || !self.recovery_due.is_some_and(|due| Instant::now() >= due)
        {
            return false;
        }
        self.recovery_due = None;
        let interrupted_prompt = self.config.active_prompt.clone();
        let prompt = self.continuation_prompt();
        match self.start_prompt(prompt.clone().into()) {
            Ok(()) => {
                self.config.active_prompt = interrupted_prompt;
                self.config.prompt_history.push(prompt);
            }
            Err(error) => {
                self.status = Status::Error;
                self.log(
                    Role::System,
                    format!("Could not continue interrupted task: {error}"),
                );
            }
        }
        true
    }

    pub(crate) fn send(&mut self, value: Value) -> Result<(), String> {
        self.connection
            .as_ref()
            .ok_or("Agent is not connected".into())
            .and_then(|connection| connection.send(value))
    }

    pub(crate) fn remember_auth_request(&mut self, request: Value) {
        self.oauth_retry = Some(OAuthRetry {
            request,
            due: None,
            attempted: false,
            accepted: false,
        });
    }

    pub(crate) fn mark_auth_request_accepted(&mut self) {
        let Some(retry) = &mut self.oauth_retry else {
            return;
        };
        if retry.request["method"] != "session/prompt" {
            return;
        }
        retry.accepted = true;
        if retry.due.take().is_some() {
            self.oauth_retry = None;
            self.active_work = false;
            self.awaiting_response = false;
            self.config.active_prompt = None;
            self.status = Status::Error;
            self.log(
                Role::System,
                "Claude accepted the message after the authentication error. Review the session before retrying.",
            );
        }
    }

    pub(crate) fn defer_oauth_retry(&mut self, id: u64, message: &str) -> bool {
        const LOCK_ERROR: &str = "Failed to refresh OAuth token: another Claude Code process is refreshing it or exited mid-refresh";
        if !message.contains(LOCK_ERROR) || self.cancel_requested {
            return false;
        }
        let Some(retry) = &mut self.oauth_retry else {
            return false;
        };
        if retry.request["id"].as_u64() != Some(id)
            || retry.attempted
            || (id >= 3 && (!self.awaiting_response || retry.accepted))
        {
            return false;
        }
        retry.attempted = true;
        retry.due = Some(Instant::now() + Duration::from_secs(60));
        self.log(
            Role::System,
            "Claude Code authentication is busy. Retrying this request in one minute.",
        );
        true
    }

    pub(crate) fn retry_oauth_request(&mut self) -> bool {
        let Some(retry) = self.oauth_retry.as_ref() else {
            return false;
        };
        if !retry.due.is_some_and(|due| Instant::now() >= due) {
            return false;
        }
        if self.status == Status::Error || self.connection.is_none() {
            self.oauth_retry = None;
            return false;
        }
        let mut request = retry.request.clone();
        let prompt = request["method"] == "session/prompt";
        if prompt {
            request["id"] = json!(self.next_request_id);
        }
        match self.send(request.clone()) {
            Ok(()) => {
                if prompt {
                    self.next_request_id += 1;
                    self.active_work = true;
                    self.awaiting_response = true;
                    self.status = Status::Working;
                }
                if let Some(retry) = &mut self.oauth_retry {
                    retry.request = request;
                    retry.due = None;
                }
            }
            Err(error) => {
                self.oauth_retry = None;
                self.status = Status::Error;
                self.log(
                    Role::System,
                    format!("Could not retry Claude request: {error}"),
                );
            }
        }
        true
    }

    pub(crate) fn start_prompt(&mut self, prompt: Prompt) -> Result<(), String> {
        let session_id = self.session_id.clone().ok_or("Agent is still connecting")?;
        if !prompt.images.is_empty() && !self.accepts_images {
            return Err("This agent does not accept images.".into());
        }
        let id = self.next_request_id;
        let agent_prompt = prompt_for_agent(&prompt.text);
        let agent_prompt = if self.config.fork_pending {
            fork_prompt(&self.messages, &agent_prompt)
        } else {
            agent_prompt
        };
        let images = prompt
            .images
            .iter()
            .map(|image| {
                self.images
                    .base64(image)
                    .map(|data| json!({"type":"image","mimeType":image.mime_type,"data":data}))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let blocks: Vec<Value> = (!agent_prompt.is_empty() || images.is_empty())
            .then(|| json!({"type":"text","text":agent_prompt}))
            .into_iter()
            .chain(images)
            .collect();
        let request = json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{
            "sessionId":session_id,"prompt":blocks
        }});
        self.send(request.clone())?;
        self.remember_auth_request(request);
        self.next_request_id += 1;
        self.config.fork_pending = false;
        self.config.fork_source = None;
        self.config.was_working = false;
        self.config.active_prompt = Some(prompt.text.clone());
        self.recovery_due = None;
        if self.protocol == Some(ProtocolVersion::V1) {
            self.push_message(Role::User, prompt.text, prompt.images);
        }
        self.active_work = true;
        self.awaiting_response = true;
        self.cancel_requested = false;
        self.config.session_has_activity = true;
        self.status = Status::Working;
        Ok(())
    }

    pub(crate) fn start_next_queued_prompt(&mut self) -> bool {
        if self.active_work
            || self.restoring.is_some()
            || self.config.was_working
            || !matches!(self.status, Status::Idle | Status::Done)
        {
            return false;
        }
        let Some(prompt) = self.config.pending_prompts.first().cloned() else {
            return false;
        };
        match self.start_prompt(prompt) {
            Ok(()) => {
                self.config.pending_prompts.remove(0);
            }
            Err(error) => {
                self.status = Status::Error;
                self.log(
                    Role::System,
                    format!("Could not send queued message: {error}"),
                );
            }
        }
        true
    }

    pub(crate) fn log(&mut self, role: Role, text: impl Into<String>) {
        self.push_message(role, text.into(), Vec::new());
    }

    fn push_message(&mut self, role: Role, text: String, images: Vec<ChatImage>) {
        if role != Role::System {
            self.config.session_has_activity = true;
        }
        self.messages.push(ChatEntry {
            role,
            key: None,
            text,
            images,
        });
    }

    #[cfg(test)]
    pub(crate) fn upsert_message(
        &mut self,
        role: Role,
        id: &str,
        content: Option<&Value>,
        append: bool,
    ) {
        let (text, images) = content.map(protocol::message_content).unwrap_or_default();
        let (text, images) = self.save_images(text, images);
        self.upsert_message_text(role, id, text, images, append);
    }

    fn save_images(&self, text: String, images: Vec<ImageData>) -> (String, Vec<ChatImage>) {
        let saved: Vec<ChatImage> = images
            .iter()
            .filter_map(|image| self.images.save_base64(&image.mime_type, &image.data).ok())
            .collect();
        let text = with_image_placeholders(&text, images.len() - saved.len());
        (text, saved)
    }

    fn upsert_message_text(
        &mut self,
        role: Role,
        id: &str,
        text: String,
        images: Vec<ChatImage>,
        append: bool,
    ) {
        self.config.session_has_activity = true;
        let key = format!("message:{id}");
        let entry = if let Some(index) = self
            .messages
            .iter()
            .position(|entry| entry.key.as_deref() == Some(&key))
        {
            &mut self.messages[index]
        } else {
            self.messages.push(ChatEntry {
                role,
                key: Some(key),
                text: String::new(),
                images: Vec::new(),
            });
            self.messages.last_mut().unwrap()
        };
        if append {
            entry.text.push_str(&text);
            entry.images.extend(images);
        } else {
            entry.text = text;
            entry.images = images;
        }
    }

    pub(crate) fn upsert_tool_call(&mut self, update: &Value) {
        let Some(id) = update["toolCallId"].as_str() else {
            if let Some(title) = update["title"].as_str() {
                self.log(Role::Tool, title);
            }
            return;
        };
        self.config.session_has_activity = true;
        let key = format!("tool:{id}");
        let existing = self
            .messages
            .iter()
            .position(|entry| entry.key.as_deref() == Some(&key));
        let previous = existing.map(|index| tool_title_and_status(&self.messages[index].text));
        let title = update["title"]
            .as_str()
            .or_else(|| previous.map(|(title, _)| title))
            .unwrap_or("Using tool");
        let status = update["status"]
            .as_str()
            .or_else(|| previous.and_then(|(_, status)| status));
        let text = if let Some(status) = status {
            format!("{title} · {status}")
        } else {
            title.to_owned()
        };
        if let Some(index) = existing {
            let details = self.messages[index]
                .text
                .split_once('\n')
                .map(|(_, details)| details.to_owned());
            self.messages[index].text = if let Some(details) = details {
                format!("{text}\n{details}")
            } else {
                text
            };
        } else {
            self.messages.push(ChatEntry {
                role: Role::Tool,
                key: Some(key),
                text,
                images: Vec::new(),
            });
        }
    }

    pub(crate) fn handle_prompt_response(&mut self, result: &Value) {
        if self.protocol == Some(ProtocolVersion::V2) {
            if let Err(error) = serde_json::from_value::<v2::PromptResponse>(result.clone()) {
                self.finish_turn(Status::Error);
                self.log(
                    Role::System,
                    format!("Invalid ACP v2 prompt response: {error}"),
                );
            }
        } else {
            self.finish_turn(Status::Done);
            if result["stopReason"].as_str() == Some("cancelled") {
                self.log(Role::System, "Turn cancelled");
            }
        }
    }
}

pub(crate) fn content_text(content: &Value) -> String {
    if let Some(items) = content.as_array() {
        return items
            .iter()
            .map(content_text)
            .collect::<Vec<_>>()
            .join("\n");
    }
    match content["type"].as_str() {
        Some("text") => content["text"].as_str().unwrap_or_default().to_owned(),
        Some("resource_link") => content["uri"].as_str().unwrap_or("[resource]").to_owned(),
        Some("image") => "[image]".into(),
        Some("audio") => "[audio]".into(),
        Some("resource") => "[resource]".into(),
        Some(kind) => format!("[{kind}]"),
        None => String::new(),
    }
}

pub(crate) fn tool_title_and_status(text: &str) -> (&str, Option<&str>) {
    let headline = text.lines().next().unwrap_or(text);
    let Some((title, status)) = headline.rsplit_once(" · ") else {
        return (headline, None);
    };
    if matches!(status, "pending" | "in_progress" | "completed" | "failed") {
        (title, Some(status))
    } else {
        (headline, None)
    }
}

fn update_config_options(agent: &mut SessionController, options: &Value) {
    if let Some(option) = model_option(options) {
        agent.model = Some(option.label());
        agent.model_option = Some(option);
    } else {
        agent.model_option = None;
    }
    agent.effort_option = effort_option(options);
    if let Some(option) = mode_option(options) {
        agent.mode_option = Some(option);
        agent.legacy_modes = false;
    } else if !agent.legacy_modes {
        agent.mode_option = None;
    }
    agent.collaboration_option = collaboration_option(options);
}

/// Applies the config options and session modes from a session response.
fn update_session_settings(agent: &mut SessionController, result: &Value) {
    update_config_options(agent, &result["configOptions"]);
    update_session_modes(agent, &result["modes"]);
}

/// Only form construction and cancellation need a UI host.
pub(crate) enum UiRequest {
    Elicitation { request_id: Value, params: Value },
    CancelElicitation(Value),
}

impl SessionController {
    pub(crate) fn start_new_session(agent: &mut SessionController, path: &Path) {
        agent.restoring = None;
        agent.active_work = false;
        agent.awaiting_response = false;
        agent.cancel_requested = false;
        agent.status = Status::Connecting;
        agent.recovery_due = None;
        agent.config.was_working = false;
        agent.config.active_prompt = None;
        agent.oauth_retry = None;
        agent.session_id = None;
        agent.config.session_id = None;
        agent.config.session_has_activity = false;
        let request = json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{
            "cwd":path,"mcpServers":[]
        }});
        if let Err(error) = agent.send(request.clone()) {
            agent.status = Status::Error;
            agent.log(Role::System, error);
        } else {
            agent.remember_auth_request(request);
        }
    }

    pub(crate) fn resume_session(
        agent: &mut SessionController,
        path: &Path,
        mode: RestoreMode,
        session_id: String,
    ) {
        agent.session_id = Some(session_id.clone());
        agent.restoring = Some(mode);
        let method = match mode {
            RestoreMode::Resume => "session/resume",
            RestoreMode::Load => "session/load",
        };
        let mut request = json!({"jsonrpc":"2.0","id":2,"method":method,"params":{
            "sessionId":session_id,"cwd":path,"mcpServers":[]
        }});
        if agent.protocol == Some(ProtocolVersion::V2) {
            request["params"]["replayFrom"] = json!({"type":"start"});
        }
        agent.messages.clear();
        agent.config.prompt_history.clear();
        if let Err(error) = agent.send(request.clone()) {
            agent.status = Status::Error;
            agent.log(Role::System, error);
        } else {
            agent.remember_auth_request(request);
        }
    }

    pub(crate) fn handle_message(&mut self, path: &Path, value: &Value) -> Option<UiRequest> {
        let agent = self;
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            match method {
                "session/update" => agent.handle_update(value),
                "$/cancel_request" => {
                    return Some(UiRequest::CancelElicitation(
                        value["params"]["requestId"].clone(),
                    ));
                }
                "session/request_permission" => {
                    let request_id = value.get("id").cloned()?;
                    let params = &value["params"];
                    if let Some(session_id) = &agent.session_id
                        && params["sessionId"].as_str() != Some(session_id)
                    {
                        return None;
                    }
                    let title = params["title"]
                        .as_str()
                        .or_else(|| params["toolCall"]["title"].as_str())
                        .unwrap_or("Agent requests permission")
                        .to_owned();
                    let description = params["description"]
                        .as_str()
                        .or_else(|| params["subject"]["command"].as_str())
                        .map(str::to_owned);
                    let options: Vec<(String, String)> = params["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|option| {
                            Some((
                                option["optionId"].as_str()?.to_owned(),
                                option["name"].as_str()?.to_owned(),
                            ))
                        })
                        .collect();
                    if options.is_empty() {
                        let _ = agent.send(json!({"jsonrpc":"2.0","id":request_id,"result":{"outcome":{"outcome":"cancelled"}}}));
                    } else {
                        agent.permissions.push(Permission {
                            request_id,
                            title,
                            description,
                            options,
                        });
                    }
                }
                "elicitation/create" => {
                    let request_id = value.get("id")?.clone();
                    let params = &value["params"];
                    if params["sessionId"].as_str() != agent.session_id.as_deref() {
                        let _ = agent.send(json!({"jsonrpc":"2.0","id":request_id,"error":{"code":-32602,"message":"Unknown session"}}));
                    } else {
                        return Some(UiRequest::Elicitation {
                            request_id,
                            params: params.clone(),
                        });
                    }
                }
                _ => {
                    if let Some(id) = value.get("id") {
                        let _ = agent.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not supported by this client"}}));
                    }
                }
            }
            return None;
        }
        let id = value.get("id").and_then(Value::as_u64)?;
        let config_kind = [
            ConfigOptionKind::Model,
            ConfigOptionKind::Effort,
            ConfigOptionKind::Mode,
            ConfigOptionKind::Collaboration,
        ]
        .into_iter()
        .find(|kind| {
            agent
                .pending_setting(*kind)
                .as_ref()
                .is_some_and(|(pending_id, _)| *pending_id == id)
        });
        if let Some(kind) = config_kind {
            let (_, selected) = agent.pending_setting(kind).take().unwrap();
            if let Some(error) = value.get("error") {
                let message = error["message"].as_str().unwrap_or("unknown error");
                agent.log(Role::System, format!("Could not change setting: {message}"));
            } else if value["result"]["configOptions"].is_array() {
                update_config_options(agent, &value["result"]["configOptions"]);
            } else if let Some(option) = agent.setting_option(kind) {
                option.current = selected;
                if kind == ConfigOptionKind::Model {
                    agent.model = Some(option.label());
                }
            }
            return None;
        }
        if let Some(error) = value.get("error") {
            let message = error["message"].as_str().unwrap_or("unknown error");
            if agent.defer_oauth_retry(id, message) {
                return None;
            }
            if agent
                .oauth_retry
                .as_ref()
                .is_some_and(|retry| retry.request["id"] == id)
            {
                agent.oauth_retry = None;
            }
            if id == 2 && agent.restoring.take().is_some() {
                agent.log(
                    Role::System,
                    format!("Could not restore the previous session: {message}"),
                );
                agent.status = Status::Error;
                agent.session_id = agent.config.session_id.clone();
                agent.recovery_due = None;
                return None;
            }
            if id >= 3 && agent.protocol == Some(ProtocolVersion::V2) {
                agent.active_work = false;
                agent.awaiting_response = false;
                agent.cancel_requested = false;
                agent.config.active_prompt = None;
                agent.status = Status::Idle;
            } else {
                agent.status = Status::Error;
            }
            let details = error["data"]["details"].as_str();
            agent.log(
                Role::System,
                match details {
                    Some(details) if !details.is_empty() => {
                        format!("ACP error: {message}\n{details}")
                    }
                    _ => format!("ACP error: {message}"),
                },
            );
            return None;
        }
        if agent
            .oauth_retry
            .as_ref()
            .is_some_and(|retry| retry.request["id"] == id)
        {
            agent.oauth_retry = None;
        }
        match id {
            1 => {
                let protocol = match value["result"]["protocolVersion"].as_u64() {
                    Some(2) => {
                        match serde_json::from_value::<v2::InitializeResponse>(
                            value["result"].clone(),
                        ) {
                            Ok(response) if response.capabilities.session.is_some() => {
                                Some(ProtocolVersion::V2)
                            }
                            Ok(_) => {
                                agent.log(
                                    Role::System,
                                    "ACP v2 agent did not advertise session support",
                                );
                                None
                            }
                            Err(error) => {
                                agent.log(
                                    Role::System,
                                    format!("Invalid ACP v2 initialization: {error}"),
                                );
                                None
                            }
                        }
                    }
                    Some(1) => Some(ProtocolVersion::V1),
                    _ => {
                        agent.log(Role::System, "Agent does not support ACP v1 or v2");
                        None
                    }
                };
                if let Some(protocol) = protocol {
                    agent.protocol = Some(protocol);
                    agent.accepts_images = accepts_images(protocol, &value["result"]);
                    if agent.config.fork_pending && agent.messages.is_empty() {
                        if let Some(source) = &agent.config.fork_source {
                            if let Some(mode) =
                                restore_mode(protocol, &value["result"]).filter(|mode| {
                                    protocol == ProtocolVersion::V2 || *mode == RestoreMode::Load
                                })
                            {
                                Self::resume_session(agent, path, mode, source.session_id.clone());
                            } else {
                                agent.status = Status::Error;
                                agent.log(Role::System, "This agent cannot reload the source conversation for this fork.");
                            }
                        } else {
                            agent.status = Status::Error;
                            agent.log(
                                Role::System,
                                "The source session for this fork is unavailable.",
                            );
                        }
                    } else if let Some(previous_session) = agent
                        .config
                        .session_id
                        .clone()
                        .filter(|_| agent.has_restorable_activity())
                    {
                        if let Some(mode) = restore_mode(protocol, &value["result"]) {
                            Self::resume_session(agent, path, mode, previous_session);
                        } else {
                            agent.status = Status::Error;
                            agent.log(Role::System, "This agent cannot restore the saved session. Create a new session to continue.");
                        }
                    } else {
                        Self::start_new_session(agent, path);
                    }
                } else {
                    agent.status = Status::Error;
                }
            }
            2 => {
                if agent.restoring.take().is_some() {
                    if agent.config.fork_pending {
                        if !agent.restore_fork_history() {
                            agent.status = Status::Error;
                            agent.log(
                                Role::System,
                                "The forked reply is no longer available in the source session.",
                            );
                            return None;
                        }
                        Self::start_new_session(agent, path);
                        return None;
                    }
                    agent.rebuild_prompt_history();
                    update_session_settings(agent, &value["result"]);
                    if agent.protocol == Some(ProtocolVersion::V1) && agent.messages.is_empty() {
                        agent.log(
                            Role::System,
                            "This agent resumed the session without reloading earlier messages.",
                        );
                    }
                    if agent.status == Status::Connecting {
                        agent.status = Status::Idle;
                    }
                    if agent.config.was_working && !agent.active_work {
                        // A live v2 agent may report running just after resume responds.
                        agent.recovery_due = Some(Instant::now() + Duration::from_secs(1));
                    }
                } else if let Some(session_id) = value["result"]["sessionId"].as_str() {
                    agent.session_id = Some(session_id.to_owned());
                    agent.config.session_id = agent.session_id.clone();
                    update_session_settings(agent, &value["result"]);
                    agent.status = Status::Idle;
                } else {
                    agent.status = Status::Error;
                    agent.log(Role::System, "Agent returned no session ID");
                }
            }
            _ => {
                agent.handle_prompt_response(&value["result"]);
            }
        }
        None
    }

    fn rebuild_prompt_history(&mut self) {
        self.config.prompt_history = self
            .messages
            .iter()
            .filter(|entry| entry.role == Role::User)
            .map(|entry| entry.text.clone())
            .chain(
                self.config
                    .pending_prompts
                    .iter()
                    .map(|prompt| prompt.text.clone()),
            )
            .collect();
    }

    fn restore_fork_history(&mut self) -> bool {
        let Some(source) = &self.config.fork_source else {
            return false;
        };
        let index = if let Some(key) = &source.reply_key {
            self.messages
                .iter()
                .position(|entry| entry.role == Role::Agent && entry.key.as_ref() == Some(key))
        } else {
            self.messages
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.role == Role::Agent)
                .nth(source.reply_index)
                .map(|(index, _)| index)
        };
        let Some(index) = index else {
            return false;
        };
        self.messages.truncate(index + 1);
        for entry in &mut self.messages {
            entry.key = None;
        }
        self.rebuild_prompt_history();
        true
    }

    pub(crate) fn disconnected(&mut self, reason: String) {
        self.oauth_retry = None;
        self.awaiting_response = false;
        self.clear_pending_settings();
        self.cancel_requested = false;
        self.permissions.clear();
        if self.status != Status::Error {
            self.status = Status::Error;
            self.log(Role::System, reason);
        }
    }
}

impl SessionController {
    fn enter_running(&mut self) {
        self.status = Status::Working;
        self.active_work = true;
        self.config.was_working = false;
        self.recovery_due = None;
    }

    fn finish_turn(&mut self, status: Status) {
        self.status = status;
        self.active_work = false;
        self.awaiting_response = false;
        self.cancel_requested = false;
        self.config.active_prompt = None;
    }

    pub(crate) fn handle_update(&mut self, value: &Value) {
        let event = match decode_update(self.protocol, self.session_id.as_deref(), value) {
            Ok(Some(event)) => event,
            Ok(None) => return,
            Err(error) => {
                self.log(Role::System, error);
                return;
            }
        };
        match event {
            SessionUpdate::Title(title) => self.config.title = title,
            SessionUpdate::Context(used, size) => self.context = Some((used, size)),
            SessionUpdate::ConfigOptions(options) => update_config_options(self, &options),
            SessionUpdate::Mode(mode) => {
                if let Some(option) = &mut self.mode_option {
                    option.current = mode;
                }
            }
            SessionUpdate::Commands(commands) => self.config.available_commands = commands,
            SessionUpdate::Running => self.enter_running(),
            SessionUpdate::Idle(reason) => {
                if self.oauth_retry.as_ref().is_some_and(|retry| {
                    retry.due.is_some() && retry.request["method"] == "session/prompt"
                }) {
                    return;
                }
                self.finish_turn(if self.active_work {
                    Status::Done
                } else {
                    Status::Idle
                });
                if let Some(reason) = reason
                    && reason != "end_turn"
                {
                    self.log(Role::System, format!("Stopped: {reason}"));
                }
            }
            SessionUpdate::Message {
                role,
                id,
                text,
                images,
                append,
            } => {
                if role == Role::User {
                    self.mark_auth_request_accepted();
                } else {
                    self.awaiting_response = false;
                }
                let (text, images) = self.save_images(text, images);
                if let Some(id) = id {
                    self.upsert_message_text(role, &id, text, images, append);
                } else if let Some(last) = self.messages.last_mut()
                    && last.role == role
                {
                    last.text.push_str(&text);
                    last.images.extend(images);
                } else {
                    self.push_message(role, text, images);
                }
            }
            SessionUpdate::Tool(update) => {
                self.awaiting_response = false;
                self.upsert_tool_call(&update);
            }
            SessionUpdate::ToolChunk { id, text } => {
                self.awaiting_response = false;
                let key = format!("tool:{id}");
                if let Some(entry) = self
                    .messages
                    .iter_mut()
                    .find(|entry| entry.key.as_deref() == Some(&key))
                    && !text.is_empty()
                {
                    entry.text.push('\n');
                    entry.text.push_str(&text);
                }
            }
        }
    }
}
