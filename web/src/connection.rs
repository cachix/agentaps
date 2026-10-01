//! Iroh transport and polling own no GPUI state.
use crate::browser::{browserClientName, pageHidden, pollDelay};
use agentaps_control_protocol::{
    ALPN, AgentOption, Command, MAX_RESPONSE_BYTES, Project, Request, Response,
};
use async_channel::{Receiver, Sender};
use iroh::{
    Endpoint, EndpointId, RelayConfig, RelayMap, RelayMode, RelayUrl, TransportAddr,
    address_lookup::{
        AddressLookup, AddressLookupBuilder, AddressLookupBuilderError, EndpointData, EndpointInfo,
        Error as AddressLookupError, Item, PkarrPublisher, PkarrResolver,
    },
    endpoint::{BindError, presets},
};
use n0_future::{StreamExt, boxed::BoxStream};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

// Safari cannot connect to hostnames with a trailing dot, and n0's default relay URLs all end in
// one. Use dot-free copies for this browser's relays and for relays the desktop publishes.
fn without_trailing_dot(relay: &RelayUrl) -> RelayUrl {
    let Some(host) = relay.host_str().and_then(|host| host.strip_suffix('.')) else {
        return relay.clone();
    };
    let mut url = (**relay).clone();
    match url.set_host(Some(host)) {
        Ok(()) => RelayUrl::from(url),
        Err(_) => relay.clone(),
    }
}

#[derive(Debug)]
struct DotlessRelays<T>(T);

#[derive(Debug)]
struct DotlessRelaysBuilder<T>(T);

impl<T: AddressLookupBuilder> AddressLookupBuilder for DotlessRelaysBuilder<T> {
    fn into_address_lookup(
        self,
        endpoint: &Endpoint,
    ) -> Result<impl AddressLookup, AddressLookupBuilderError> {
        Ok(DotlessRelays(self.0.into_address_lookup(endpoint)?))
    }
}

impl<T: AddressLookup> AddressLookup for DotlessRelays<T> {
    fn resolve(
        &self,
        endpoint_id: EndpointId,
    ) -> Option<BoxStream<Result<Item, AddressLookupError>>> {
        let items = self.0.resolve(endpoint_id)?.map(|item| {
            item.map(|item| {
                let mut data = EndpointData::new(
                    item.addrs()
                        .map(|addr| match addr {
                            TransportAddr::Relay(relay) => {
                                TransportAddr::Relay(without_trailing_dot(relay))
                            }
                            addr => addr.clone(),
                        })
                        .collect(),
                );
                data.set_user_data(item.user_data());
                Item::new(
                    EndpointInfo::from_parts(item.endpoint_id(), data),
                    item.provenance(),
                    item.last_updated(),
                )
            })
        });
        Some(Box::pin(items))
    }
}

async fn bind_endpoint() -> Result<Endpoint, BindError> {
    let relays: Vec<RelayUrl> = iroh::defaults::prod::default_relay_map().urls();
    let relays = relays
        .iter()
        .map(|relay| RelayConfig::from(without_trailing_dot(relay)));
    Endpoint::builder(presets::Minimal)
        .address_lookup(PkarrPublisher::n0_dns())
        .address_lookup(DotlessRelaysBuilder(PkarrResolver::n0_dns()))
        .relay_mode(RelayMode::Custom(RelayMap::from_iter(relays)))
        .bind()
        .await
}

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
    let endpoint = bind_endpoint()
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
    let endpoint =
        match futures_lite::future::race(async { bind_endpoint().await.map(Some) }, async {
            let _ = cancel.recv().await;
            Ok(None)
        })
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
