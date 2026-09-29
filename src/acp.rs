use crate::discovery::find_executable;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

#[derive(Debug)]
pub enum Event {
    Message { agent_id: u64, value: Value },
    Disconnected { agent_id: u64, reason: String },
}

pub struct Connection {
    child: Child,
    outgoing: Sender<Value>,
}

impl Connection {
    pub fn spawn(
        agent_id: u64,
        command: &[String],
        cwd: &Path,
        ssh_host: Option<&str>,
        events: Sender<Event>,
    ) -> Result<Self, String> {
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
        if ssh_host.is_none() && uses_claude_agent_acp(command) {
            let claude = std::env::var_os("CLAUDE_CODE_EXECUTABLE")
                .map(std::path::PathBuf::from)
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
                            if events.send(Event::Message { agent_id, value }).is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = events.send(Event::Disconnected {
                                agent_id,
                                reason: format!("Invalid ACP message: {error}"),
                            });
                            return;
                        }
                    },
                    Err(error) => {
                        let _ = events.send(Event::Disconnected {
                            agent_id,
                            reason: format!("Agent stream failed: {error}"),
                        });
                        return;
                    }
                }
            }
            let _ = events.send(Event::Disconnected {
                agent_id,
                reason: "Agent disconnected".into(),
            });
        });
        Ok(Self { child, outgoing })
    }

    pub fn send(&self, value: Value) -> Result<(), String> {
        self.outgoing
            .send(value)
            .map_err(|_| "Agent disconnected".into())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn exchanges_json_rpc_lines_with_stdio_agent() {
        let script = "IFS= read -r request; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}'";
        let command = vec!["/bin/sh".into(), "-c".into(), script.into()];
        let (tx, rx) = mpsc::channel();
        let connection = Connection::spawn(7, &command, Path::new("/"), None, tx).unwrap();
        connection
            .send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
            .unwrap();
        let event = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match event {
            Event::Message { agent_id, value } => {
                assert_eq!(agent_id, 7);
                assert_eq!(value["result"]["protocolVersion"], 1);
            }
            Event::Disconnected { reason, .. } => panic!("Disconnected before reply: {reason}"),
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
