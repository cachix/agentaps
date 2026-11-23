use serde::{Deserialize, Serialize};

pub mod sidebar;

pub const ALPN: &[u8] = b"agentaps/control/0";
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub token: String,
    pub command: Command,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Pair,
    Snapshot,
    ReopenSession {
        agent_id: u64,
    },
    Prompt {
        agent_id: u64,
        text: String,
    },
    Cancel {
        agent_id: u64,
    },
    Permission {
        agent_id: u64,
        request_id: String,
        option_id: String,
    },
    NewSession {
        project: String,
        command: Vec<String>,
        name: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Paired {
        token: String,
    },
    Snapshot {
        projects: Vec<Project>,
        #[serde(default)]
        agent_options: Vec<AgentOption>,
        #[serde(default)]
        flat_sidebar: bool,
    },
    SessionCreated {
        agent_id: u64,
    },
    Accepted,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub path: String,
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archived_agents: Vec<Agent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentOption {
    pub name: String,
    pub command: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Agent {
    pub id: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_order: Option<usize>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub needs_attention: bool,
    pub status: String,
    pub active: bool,
    pub has_older_messages: bool,
    pub messages: Vec<Message>,
    pub permissions: Vec<Permission>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Permission {
    pub request_id: String,
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<PermissionOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub label: String,
}
