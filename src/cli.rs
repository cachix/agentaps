use crate::acp::Event;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    time::Duration,
};

const ADDRESS_ENV: &str = "AGENTAPS_CONTROL_ADDR";
// Agent shell tools commonly strip environment names containing TOKEN or SECRET.
const TOKEN_ENV: &str = "AGENTAPS_CONTROL_AUTH";
const TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_SKILL: &str = include_str!("../skills/agentaps/SKILL.md");
const SESSION_SKILL_URI: &str = "agentaps://skills/agentaps/SKILL.md";

pub(crate) fn skill_context(embed: bool) -> serde_json::Value {
    if embed {
        serde_json::json!({"type":"resource","resource":{
            "uri":SESSION_SKILL_URI,"mimeType":"text/markdown","text":SESSION_SKILL
        }})
    } else {
        serde_json::json!({"type":"text","text":SESSION_SKILL})
    }
}

pub(crate) fn is_skill_context(block: &serde_json::Value) -> bool {
    (block["type"] == "resource" && block["resource"]["uri"] == SESSION_SKILL_URI)
        || (block["type"] == "text" && block["text"] == SESSION_SKILL)
}

pub fn new_session() -> Result<String, String> {
    let address = std::env::var(ADDRESS_ENV)
        .map_err(|_| "Run agentaps new-session from a local Agentaps agent session".to_owned())?;
    let token = std::env::var(TOKEN_ENV)
        .map_err(|_| "Agentaps session control token is missing".to_owned())?;
    request(&address, &token).map_err(|error| format!("Could not open a new session: {error}"))
}

fn request(address: &str, token: &str) -> Result<String, String> {
    let address: SocketAddr = address.parse().map_err(|_| "Invalid control address")?;
    if !address.ip().is_loopback() {
        return Err("Control address must be on localhost".into());
    }
    let exchange = || -> std::io::Result<String> {
        let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        writeln!(stream, "{token} new-session")?;
        let mut response = String::new();
        BufReader::new(stream).take(512).read_line(&mut response)?;
        Ok(response.trim().to_owned())
    };
    let response = exchange().map_err(|error| error.to_string())?;
    response
        .strip_prefix("ok ")
        .map(str::to_owned)
        .ok_or_else(|| {
            if response.is_empty() {
                "Session control connection closed".into()
            } else {
                response
            }
        })
}

/// A control endpoint whose lifetime and credentials belong to one agent connection.
pub struct Server {
    address: SocketAddr,
    token: String,
    stopped: Arc<AtomicBool>,
}

impl Server {
    pub fn start(
        agent_id: u64,
        connection_id: u64,
        events: Sender<Event>,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        let token = hex::encode(iroh::SecretKey::generate().to_bytes());
        let expected = format!("{token} new-session\n");
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = stopped.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if thread_stopped.load(Ordering::Acquire) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                let mut handle = || -> std::io::Result<()> {
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                    let mut line = String::new();
                    BufReader::new(&stream).take(512).read_line(&mut line)?;
                    if line != expected {
                        return writeln!(stream, "Invalid session control request");
                    }
                    let (reply, result) = mpsc::channel();
                    if events
                        .send(Event::NewSession {
                            agent_id,
                            connection_id,
                            reply,
                        })
                        .is_err()
                    {
                        return writeln!(stream, "Agentaps workspace is closed");
                    }
                    match result.recv_timeout(TIMEOUT) {
                        Ok(Ok(id)) => writeln!(stream, "ok Opened session {id}"),
                        Ok(Err(error)) => writeln!(stream, "{error}"),
                        Err(_) => writeln!(stream, "Agentaps session is no longer available"),
                    }
                };
                let _ = handle();
            }
        });
        Ok(Self {
            address,
            token,
            stopped,
        })
    }

    pub fn configure(&self, process: &mut Command) {
        process
            .env(ADDRESS_ENV, self.address.to_string())
            .env(TOKEN_ENV, &self.token);
        // Make the running binary available even when launched from a build directory.
        if let Ok(executable) = std::env::current_exe()
            && let Some(parent) = executable.parent()
        {
            let mut paths = vec![parent.to_path_buf()];
            if let Some(path) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&path));
            }
            if let Ok(path) = std::env::join_paths(paths) {
                process.env("PATH", path);
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        // Wake the listener so dropping an agent also releases its listening socket.
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_reaches_the_calling_connection_and_waits_for_creation() {
        let (events, incoming) = mpsc::channel();
        let server = Server::start(7, 42, events).unwrap();
        let address = server.address.to_string();
        let token = server.token.clone();
        let client = std::thread::spawn(move || request(&address, &token));
        let event = incoming.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(event.source(), (7, 42));
        let Event::NewSession { reply, .. } = event else {
            panic!("Expected new session request")
        };
        reply.send(Ok(8)).unwrap();
        assert_eq!(client.join().unwrap().unwrap(), "Opened session 8");
    }

    #[test]
    fn rejects_invalid_credentials_without_creating_a_session() {
        let (events, incoming) = mpsc::channel();
        let server = Server::start(7, 42, events).unwrap();
        assert!(request(&server.address.to_string(), "wrong").is_err());
        assert!(incoming.try_recv().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn control_credentials_reach_agent_shell() {
        let (events, incoming) = mpsc::channel();
        let server = Server::start(7, 42, events).unwrap();
        let mut shell = Command::new("/bin/sh");
        shell.args([
            "-c",
            "printf '%s\\n%s\\n' \"$AGENTAPS_CONTROL_ADDR\" \"$AGENTAPS_CONTROL_AUTH\"",
        ]);
        server.configure(&mut shell);
        let output = shell.output().unwrap();
        assert!(output.status.success());
        let environment = String::from_utf8(output.stdout).unwrap();
        let mut lines = environment.lines();
        let address = lines.next().unwrap().to_owned();
        let token = lines.next().unwrap().to_owned();
        let client = std::thread::spawn(move || request(&address, &token));
        let Event::NewSession { reply, .. } = incoming.recv_timeout(TIMEOUT).unwrap() else {
            panic!("Expected new session request")
        };
        reply.send(Ok(8)).unwrap();
        assert_eq!(client.join().unwrap().unwrap(), "Opened session 8");
    }

    #[test]
    fn reports_workspace_errors() {
        let (events, incoming) = mpsc::channel();
        let server = Server::start(7, 42, events).unwrap();
        let address = server.address.to_string();
        let token = server.token.clone();
        let client = std::thread::spawn(move || request(&address, &token));
        let Event::NewSession { reply, .. } = incoming.recv_timeout(TIMEOUT).unwrap() else {
            panic!("Expected new session request")
        };
        reply.send(Err("Session is archived".into())).unwrap();
        assert_eq!(client.join().unwrap().unwrap_err(), "Session is archived");
    }
}
