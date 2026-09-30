//! Iroh transport and polling own no GPUI state.
use crate::browser::{browserClientName, pageHidden, pollDelay};
use agentaps_control_protocol::{
    ALPN, AgentOption, Command, MAX_RESPONSE_BYTES, Project, Request, Response,
};
use async_channel::{Receiver, Sender};
use iroh::{Endpoint, EndpointId, endpoint::presets};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

pub(super) async fn request(
    endpoint: &Endpoint,
    remote: EndpointId,
    token: &str,
    command: Command,
) -> Result<Response, String> {
    let connection = endpoint
        .connect(remote, ALPN)
        .await
        .map_err(|error| error.to_string())?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&Request {
        token: token.into(),
        client_name: matches!(&command, Command::Pair).then(browserClientName),
        command,
    })
    .map_err(|error| error.to_string())?;
    send.write_all(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    send.finish().map_err(|error| error.to_string())?;
    let bytes = recv
        .read_to_end(MAX_RESPONSE_BYTES)
        .await
        .map_err(|error| error.to_string())?;
    connection.close(0u32.into(), b"done");
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

pub(super) async fn exchange_pairing(
    remote: EndpointId,
    pairing_token: &str,
) -> Result<String, JsValue> {
    let endpoint = Endpoint::bind(presets::N0)
        .await
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let response = request(&endpoint, remote, pairing_token, Command::Pair).await;
    endpoint.close().await;
    match response {
        Ok(Response::Paired { token }) => Ok(token),
        Ok(Response::Error { message }) | Err(message) => Err(JsValue::from_str(&message)),
        _ => Err(JsValue::from_str("Unexpected pairing response")),
    }
}

pub(super) enum ClientEvent {
    Snapshot {
        projects: Vec<Project>,
        agent_options: Vec<AgentOption>,
    },
    Command {
        prompt: Option<String>,
        creating: bool,
        created: Option<u64>,
        error: Option<String>,
    },
    Error(String),
    Hidden,
}

pub(super) async fn run(
    remote: EndpointId,
    token: String,
    commands: Receiver<Command>,
    events: Sender<ClientEvent>,
    cancel: Receiver<()>,
) {
    let endpoint = match futures_lite::future::race(
        async { Endpoint::bind(presets::N0).await.map(Some) },
        async {
            let _ = cancel.recv().await;
            Ok(None)
        },
    )
    .await
    {
        Ok(Some(endpoint)) => endpoint,
        Ok(None) => return,
        Err(error) => {
            let _ = events
                .send(ClientEvent::Error(format!("Could not start Iroh: {error}")))
                .await;
            return;
        }
    };
    let work = async {
        loop {
            if pageHidden() {
                let _ = events.send(ClientEvent::Hidden).await;
                break;
            }
            while let Ok(command) = commands.try_recv() {
                let prompt = match &command {
                    Command::Prompt { text, .. } => Some(text.clone()),
                    _ => None,
                };
                let creating = matches!(&command, Command::NewSession { .. });
                let response = request(&endpoint, remote, &token, command).await;
                let created = match &response {
                    Ok(Response::SessionCreated { agent_id }) => Some(*agent_id),
                    _ => None,
                };
                let error = match response {
                    Ok(Response::Error { message }) | Err(message) => Some(message),
                    _ => None,
                };
                if events
                    .send(ClientEvent::Command {
                        prompt,
                        creating,
                        created,
                        error,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let event = match request(&endpoint, remote, &token, Command::Snapshot).await {
                Ok(Response::Snapshot {
                    projects,
                    agent_options,
                }) => Some(ClientEvent::Snapshot {
                    projects,
                    agent_options,
                }),
                Ok(Response::Error { message }) | Err(message) => Some(ClientEvent::Error(message)),
                _ => None,
            };
            if let Some(event) = event
                && events.send(event).await.is_err()
            {
                return;
            }
            let _ = JsFuture::from(pollDelay()).await;
        }
    };
    futures_lite::future::race(work, async {
        let _ = cancel.recv().await;
    })
    .await;
    endpoint.close().await;
}
