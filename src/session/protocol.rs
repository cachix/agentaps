use super::*;

pub(crate) fn parse_available_commands(update: &Value) -> Vec<SlashCommand> {
    update["availableCommands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|command| {
            let name = command["name"].as_str()?.trim().trim_start_matches('/');
            if name.is_empty() || name.chars().any(char::is_whitespace) {
                return None;
            }
            Some(SlashCommand {
                name: name.to_owned(),
                description: command["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                input: command.get("input").cloned(),
            })
        })
        .collect()
}

pub(crate) fn initialize_params() -> Value {
    let initialize = v2::InitializeRequest::new(
        ProtocolVersion::V2,
        v2::Implementation::new("agentaps", env!("CARGO_PKG_VERSION")).title("Agentaps"),
    )
    .capabilities(v2::ClientCapabilities::new().elicitation(
        v2::ElicitationCapabilities::new().form(v2::ElicitationFormCapabilities::new()),
    ));
    let mut params = json!(initialize);
    // A v1 agent reads this field when it accepts our v2 version offer.
    params["clientCapabilities"] = json!({"elicitation":{"form":{}}});
    params
}

pub(super) fn accepts_images(protocol: ProtocolVersion, initialize: &Value) -> bool {
    match protocol {
        ProtocolVersion::V2 => initialize["capabilities"]["session"]["prompt"]["image"].is_object(),
        ProtocolVersion::V1 => {
            initialize["agentCapabilities"]["promptCapabilities"]["image"].as_bool() == Some(true)
        }
        _ => false,
    }
}

pub(super) struct ImageData {
    pub(super) mime_type: String,
    pub(super) data: String,
}

pub(super) fn message_content(content: &Value) -> (String, Vec<ImageData>) {
    let blocks = content
        .as_array()
        .map_or_else(|| vec![content], |items| items.iter().collect());
    let mut text = Vec::new();
    let mut images = Vec::new();
    for block in blocks {
        match (
            block["type"].as_str(),
            block["mimeType"].as_str(),
            block["data"].as_str(),
        ) {
            (Some("image"), Some(mime_type), Some(data)) => images.push(ImageData {
                mime_type: mime_type.to_owned(),
                data: data.to_owned(),
            }),
            _ => text.push(content_text(block)),
        }
    }
    (text.join("\n"), images)
}

/// Both ACP versions become the same events before changing session state.
pub(super) enum SessionUpdate {
    Title(Option<String>),
    Context(u64, u64),
    ConfigOptions(Value),
    Mode(String),
    Commands(Vec<SlashCommand>),
    Running,
    Idle(Option<String>),
    Message {
        role: Role,
        id: Option<String>,
        text: String,
        images: Vec<ImageData>,
        append: bool,
    },
    Tool(Value),
    ToolChunk {
        id: String,
        text: String,
    },
}

pub(super) fn decode_update(
    protocol: Option<ProtocolVersion>,
    session_id: Option<&str>,
    value: &Value,
) -> Result<Option<SessionUpdate>, String> {
    let params = &value["params"];
    if session_id.is_some() && params["sessionId"].as_str() != session_id {
        return Ok(None);
    }
    let v2 = protocol == Some(ProtocolVersion::V2);
    if v2 {
        serde_json::from_value::<v2::UpdateSessionNotification>(params.clone())
            .map_err(|error| format!("Invalid ACP v2 update: {error}"))?;
    }
    let update = &params["update"];
    let event = match update["sessionUpdate"].as_str() {
        Some("session_info_update") => match update.get("title") {
            Some(Value::Null) => Some(SessionUpdate::Title(None)),
            Some(Value::String(title)) => Some(SessionUpdate::Title(
                Some(title.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|title| !title.is_empty()),
            )),
            _ => None,
        },
        Some("usage_update") => match (update["used"].as_u64(), update["size"].as_u64()) {
            (Some(used), Some(size)) if size > 0 => Some(SessionUpdate::Context(used, size)),
            _ => None,
        },
        Some("config_option_update") => Some(SessionUpdate::ConfigOptions(
            update["configOptions"].clone(),
        )),
        Some("current_mode_update") => update["currentModeId"]
            .as_str()
            .map(|mode| SessionUpdate::Mode(mode.into())),
        Some("available_commands_update") => {
            Some(SessionUpdate::Commands(parse_available_commands(update)))
        }
        Some("state_update") if v2 => match update["state"].as_str() {
            Some("running" | "requires_action") => Some(SessionUpdate::Running),
            Some("idle") => Some(SessionUpdate::Idle(
                update["stopReason"].as_str().map(str::to_owned),
            )),
            _ => None,
        },
        Some(
            kind @ ("user_message"
            | "user_message_chunk"
            | "agent_message"
            | "agent_message_chunk"
            | "agent_thought"
            | "agent_thought_chunk"),
        ) if v2 => update["messageId"].as_str().map(|id| {
            let (text, images) = update
                .get("content")
                .map(message_content)
                .unwrap_or_default();
            SessionUpdate::Message {
                role: if kind.starts_with("user_") {
                    Role::User
                } else if kind.starts_with("agent_thought") {
                    Role::Thought
                } else {
                    Role::Agent
                },
                id: Some(id.into()),
                text,
                images,
                append: kind.ends_with("_chunk"),
            }
        }),
        Some(kind @ ("user_message_chunk" | "agent_message_chunk" | "agent_thought_chunk")) => {
            let (text, images) = message_content(&update["content"]);
            (update["content"]["text"].is_string() || !images.is_empty()).then(|| {
                SessionUpdate::Message {
                    role: match kind {
                        "user_message_chunk" => Role::User,
                        "agent_thought_chunk" => Role::Thought,
                        _ => Role::Agent,
                    },
                    id: None,
                    text,
                    images,
                    append: true,
                }
            })
        }
        Some("tool_call_update") => Some(SessionUpdate::Tool(update.clone())),
        Some("tool_call") if !v2 => Some(SessionUpdate::Tool(update.clone())),
        Some("tool_call_content_chunk") if v2 => {
            update["toolCallId"]
                .as_str()
                .map(|id| SessionUpdate::ToolChunk {
                    id: id.into(),
                    text: content_text(&update["content"]["content"]),
                })
        }
        _ => None,
    };
    Ok(event)
}
