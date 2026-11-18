use crate::discovery::find_executable;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

#[derive(Debug)]
pub enum Event {
    NewSession {
        agent_id: u64,
        connection_id: u64,
        reply: Sender<Result<u64, String>>,
    },
    Message {
        agent_id: u64,
        connection_id: u64,
        value: Value,
    },
    Disconnected {
        agent_id: u64,
        connection_id: u64,
        reason: String,
    },
}

impl Event {
    pub fn source(&self) -> (u64, u64) {
        match self {
            Self::Message {
                agent_id,
                connection_id,
                ..
            }
            | Self::Disconnected {
                agent_id,
                connection_id,
                ..
            }
            | Self::NewSession {
                agent_id,
                connection_id,
                ..
            } => (*agent_id, *connection_id),
        }
    }
}

pub struct Connection {
    pub id: u64,
    child: Child,
    outgoing: Sender<Value>,
    _cli_server: Option<crate::cli::Server>,
}

impl Connection {
    pub fn spawn(
        agent_id: u64,
        command: &[String],
        cwd: &Path,
        ssh_host: Option<&str>,
        claude_executable: Option<&Path>,
        events: Sender<Event>,
    ) -> Result<Self, String> {
        static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);
        let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
        let (program, args) = command.split_first().ok_or("Agent command is empty")?;
        let mut process = if let Some(host) = ssh_host {
            let remote_command = crate::remote::agent_command(command, cwd)?;
            let mut ssh = Command::new("ssh");
            ssh.args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "ConnectTimeout=10",
                host,
            ])
            .arg(remote_command);
            ssh
        } else {
            let mut local = Command::new(program);
            local.args(args).current_dir(cwd);
            local
        };
        process
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Keep npm, the adapter, and its harness in an owned process group.
            process.process_group(0);
        }
        let cli_server = if ssh_host.is_none() {
            let server = crate::cli::Server::start(agent_id, connection_id, events.clone())
                .map_err(|error| format!("Could not start session control: {error}"))?;
            server.configure(&mut process);
            Some(server)
        } else {
            None
        };
        if ssh_host.is_none() && uses_codex_acp(command) {
            let codex = std::env::var_os("CODEX_PATH")
                .map(std::path::PathBuf::from)
                .or_else(|| find_executable("codex"));
            if let Some(codex) = codex {
                process.env("CODEX_PATH", codex);
            }
        }
        if ssh_host.is_none() && uses_claude_agent_acp(command) {
            let claude = std::env::var_os("CLAUDE_CODE_EXECUTABLE")
                .map(std::path::PathBuf::from)
                .or_else(|| claude_executable.map(Path::to_path_buf))
                .or_else(|| find_executable("claude"));
            if let Some(claude) = claude {
                configure_claude_code_env(&mut process, claude);
            }
        }
        let mut child = process.spawn().map_err(|error| {
            format!(
                "Could not start {}: {error}",
                if ssh_host.is_some() { "ssh" } else { program }
            )
        })?;
        let mut stdin = child.stdin.take().ok_or("Agent stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("Agent stdout unavailable")?;
        let (outgoing, incoming): (Sender<Value>, Receiver<Value>) = mpsc::channel();

        let writer_events = events.clone();
        thread::spawn(move || {
            for value in incoming {
                if writeln!(stdin, "{value}")
                    .and_then(|_| stdin.flush())
                    .is_err()
                {
                    let _ = writer_events.send(Event::Disconnected {
                        agent_id,
                        connection_id,
                        reason: "Could not write to agent".into(),
                    });
                    break;
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => match serde_json::from_str(&line) {
                        Ok(value) => {
                            if events
                                .send(Event::Message {
                                    agent_id,
                                    connection_id,
                                    value,
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = events.send(Event::Disconnected {
                                agent_id,
                                connection_id,
                                reason: format!("Invalid ACP message: {error}"),
                            });
                            return;
                        }
                    },
                    Err(error) => {
                        let _ = events.send(Event::Disconnected {
                            agent_id,
                            connection_id,
                            reason: format!("Agent stream failed: {error}"),
                        });
                        return;
                    }
                }
            }
            let _ = events.send(Event::Disconnected {
                agent_id,
                connection_id,
                reason: "Agent disconnected".into(),
            });
        });
        Ok(Self {
            id: connection_id,
            child,
            outgoing,
            _cli_server: cli_server,
        })
    }

    pub fn send(&self, value: Value) -> Result<(), String> {
        self.outgoing
            .send(value)
            .map_err(|_| "Agent disconnected".into())
    }

    pub fn session_skill(&self, embed: bool) -> Option<Value> {
        self._cli_server
            .as_ref()
            .map(|_| crate::cli::skill_context(embed))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: spawn created a group whose ID is the child's PID. Signal it
        // before wait reaps that PID, so it cannot have been reused by another
        // process. Descendants may still be alive after the wrapper exits.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Child::kill only terminates the npm wrapper on Windows.
            let _ = Command::new("taskkill.exe")
                .args(["/F", "/T", "/PID", &self.child.id().to_string()])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn uses_codex_acp(command: &[String]) -> bool {
    command.iter().any(|part| part.contains("codex-acp"))
}

fn uses_claude_agent_acp(command: &[String]) -> bool {
    command.iter().any(|part| part.contains("claude-agent-acp"))
}

fn configure_claude_code_env(process: &mut Command, executable: std::path::PathBuf) {
    process
        .env("CLAUDE_CODE_EXECUTABLE", executable)
        .env_remove("ANTHROPIC_API_KEY");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use serde_json::json;
    #[cfg(unix)]
    use std::time::Duration;

    #[cfg(unix)]
    fn process_running(pid: libc::pid_t) -> bool {
        #[cfg(target_os = "linux")]
        if std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit_once(") ")
                .is_some_and(|(_, tail)| tail.starts_with('Z'))
        }) {
            return false;
        }
        // SAFETY: signal zero only checks whether the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[cfg(unix)]
    fn assert_process_stopped(pid: libc::pid_t) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while process_running(pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "process {pid} survived connection shutdown"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    fn tree_connection(exit_wrapper: bool) -> (Connection, Receiver<Event>, libc::pid_t) {
        let script = format!(
            "sleep 60 & child=$!; printf '{{\"child\":%s}}\\n' \"$child\"; {}",
            if exit_wrapper { "exit 0" } else { "wait" }
        );
        let (tx, rx) = mpsc::channel();
        let connection = Connection::spawn(
            7,
            &["/bin/sh".into(), "-c".into(), script],
            Path::new("/"),
            None,
            None,
            tx,
        )
        .unwrap();
        let Event::Message { value, .. } = rx.recv_timeout(Duration::from_secs(3)).unwrap() else {
            panic!("agent did not report its child");
        };
        (
            connection,
            rx,
            value["child"].as_i64().unwrap() as libc::pid_t,
        )
    }

    #[test]
    #[cfg(unix)]
    fn dropping_connection_stops_wrapper_and_children_without_stopping_other_agents() {
        let (connection, events, child) = tree_connection(false);
        let (other, _, other_child) = tree_connection(false);
        let wrapper = connection.child.id() as libc::pid_t;
        assert!(process_running(child));
        drop(connection);
        assert_process_stopped(wrapper);
        assert_process_stopped(child);
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(3)).unwrap(),
            Event::Disconnected { .. }
        ));
        assert!(process_running(other.child.id() as libc::pid_t));
        assert!(process_running(other_child));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn dropping_connection_stops_children_after_wrapper_exits() {
        let (connection, events, child) = tree_connection(true);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while process_running(connection.child.id() as libc::pid_t) {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(process_running(child));
        drop(connection);
        assert_process_stopped(child);
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(3)).unwrap(),
            Event::Disconnected { .. }
        ));
    }

    #[test]
    #[cfg(unix)]
    fn exchanges_json_rpc_lines_with_stdio_agent() {
        let script = "IFS= read -r request; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}'";
        let command = vec!["/bin/sh".into(), "-c".into(), script.into()];
        let (tx, rx) = mpsc::channel();
        let connection = Connection::spawn(7, &command, Path::new("/"), None, None, tx).unwrap();
        connection
            .send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
            .unwrap();
        let event = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match event {
            Event::Message {
                agent_id, value, ..
            } => {
                assert_eq!(agent_id, 7);
                assert_eq!(value["result"]["protocolVersion"], 1);
            }
            Event::Disconnected { reason, .. } => panic!("Disconnected before reply: {reason}"),
            Event::NewSession { .. } => panic!("Unexpected new session request"),
        }
    }

    #[test]
    fn detects_codex_acp_wrappers() {
        for command in [
            vec!["codex-acp".into()],
            vec!["/opt/bin/codex-acp".into()],
            vec![
                "npx".into(),
                "-y".into(),
                "--prefer-offline".into(),
                "@agentclientprotocol/codex-acp".into(),
            ],
        ] {
            assert!(uses_codex_acp(&command));
        }
        assert!(!uses_codex_acp(&["codex".into()]));
        assert!(!uses_codex_acp(&["claude-agent-acp".into()]));
    }

    #[test]
    #[cfg(unix)]
    fn codex_adapter_uses_installed_cli_before_its_bundled_cli() {
        use std::os::unix::fs::PermissionsExt;

        const CHILD: &str = "AGENTAPS_CODEX_EXECUTABLE_TEST_ROOT";
        const EXPECTED: &str = "AGENTAPS_CODEX_EXECUTABLE_TEST_EXPECTED";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let command = vec![
                root.join("codex-acp").to_string_lossy().into_owned(),
                root.join("bundled").to_string_lossy().into_owned(),
            ];
            let (events_tx, events_rx) = mpsc::channel();
            let _connection = Connection::spawn(7, &command, &root, None, None, events_tx).unwrap();
            let Event::Message { value, .. } =
                events_rx.recv_timeout(Duration::from_secs(3)).unwrap()
            else {
                panic!("adapter did not report its Codex executable");
            };
            assert_eq!(value["codex"], std::env::var(EXPECTED).unwrap());
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let shell = find_executable("sh").unwrap();
        for name in ["installed", "bundled", "override"] {
            let bin = root.join(name);
            std::fs::create_dir(&bin).unwrap();
            let executable = bin.join("codex");
            std::fs::write(
                &executable,
                format!(
                    "#!{}\nprintf '%s\\n' '{{\"codex\":\"{name}\"}}'\n",
                    shell.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let adapter = root.join("codex-acp");
        std::fs::write(
            &adapter,
            format!(
                "#!{}\nPATH=\"$1\"\nif test -n \"$CODEX_PATH\"; then exec \"$CODEX_PATH\" app-server; else exec codex app-server; fi\n",
                shell.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o755)).unwrap();

        for (path, codex_override, expected) in [
            ("installed", None, "installed"),
            ("installed", Some("override"), "override"),
            ("missing", None, "bundled"),
        ] {
            let mut process = Command::new(std::env::current_exe().unwrap());
            process
                .args([
                    "--exact",
                    "acp::tests::codex_adapter_uses_installed_cli_before_its_bundled_cli",
                    "--nocapture",
                ])
                .env(CHILD, root)
                .env(EXPECTED, expected)
                .env("PATH", root.join(path))
                .env_remove("CODEX_PATH");
            if let Some(name) = codex_override {
                process.env("CODEX_PATH", root.join(name).join("codex"));
            }
            let result = process.output().unwrap();
            assert!(
                result.status.success(),
                "{expected}: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn detects_claude_agent_acp_wrappers() {
        assert!(uses_claude_agent_acp(&[
            "npx".into(),
            "-y".into(),
            "@agentclientprotocol/claude-agent-acp".into(),
        ]));
        assert!(uses_claude_agent_acp(&["claude-agent-acp".into()]));
        assert!(!uses_claude_agent_acp(&["claude-code-acp".into()]));
    }

    #[test]
    fn configures_claude_code_auth_for_the_adapter() {
        let mut process = Command::new("claude-agent-acp");
        configure_claude_code_env(&mut process, "/opt/homebrew/bin/claude".into());
        let envs: Vec<_> = process.get_envs().collect();
        assert!(envs.iter().any(|(name, value)| {
            *name == std::ffi::OsStr::new("CLAUDE_CODE_EXECUTABLE")
                && *value == Some(std::ffi::OsStr::new("/opt/homebrew/bin/claude"))
        }));
        assert!(envs.iter().any(|(name, value)| {
            *name == std::ffi::OsStr::new("ANTHROPIC_API_KEY") && value.is_none()
        }));
    }
}
