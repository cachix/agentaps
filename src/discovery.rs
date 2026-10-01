use crate::config::KnownAgent;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentChoice {
    pub name: String,
    pub detail: String,
    pub command: Vec<String>,
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    fn is_executable(path: &Path) -> bool {
        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        if !metadata.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    }
    fn find_at(path: &Path) -> Option<PathBuf> {
        if is_executable(path) {
            return Some(path.to_path_buf());
        }
        #[cfg(windows)]
        if path.extension().is_none() {
            let extensions = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
            for extension in extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
            {
                let candidate = path.with_extension(extension.trim_start_matches('.'));
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
        None
    }
    let path = Path::new(name);
    if path.components().count() > 1 {
        return find_at(path);
    }
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find_map(|path| find_at(&path))
}

fn executable(name: &str) -> bool {
    find_executable(name).is_some()
}

pub fn installed_agents(executables: &BTreeMap<KnownAgent, String>) -> Vec<AgentChoice> {
    let mut agents: Vec<_> = executables
        .iter()
        .map(|(agent, path)| AgentChoice {
            name: agent_name(*agent).into(),
            detail: "Set in settings".into(),
            command: std::iter::once(path.clone())
                .chain(agent_args(*agent).iter().map(|arg| (*arg).into()))
                .collect(),
        })
        .collect();
    let configured = |name: &str| executables.keys().any(|agent| agent_name(*agent) == name);
    let mut add = |name: &str, detail: &str, command: &[&str]| {
        if configured(name) {
            return;
        }
        if let Some(executable) = find_executable(command[0]) {
            let program = if cfg!(windows) {
                executable.to_string_lossy().into_owned()
            } else {
                command[0].to_owned()
            };
            let command = std::iter::once(program)
                .chain(command[1..].iter().map(|part| (*part).into()))
                .collect();
            agents.push(AgentChoice {
                name: name.into(),
                detail: detail.into(),
                command,
            });
        }
    };
    add("Codex", "Installed ACP adapter", &["codex-acp"]);
    add("Claude", "Installed ACP adapter", &["claude-agent-acp"]);
    add("Claude", "Installed ACP adapter", &["claude-code-acp"]);
    add("Gemini CLI", "Native ACP mode", &["gemini", "--acp"]);
    add("OpenCode", "Native ACP mode", &["opencode", "acp"]);
    if let Some(npx_path) = find_executable("npx") {
        let npx = if cfg!(windows) {
            npx_path.to_string_lossy().into_owned()
        } else {
            "npx".to_owned()
        };
        if executable("codex") && !agents.iter().any(|agent| agent.name == "Codex") {
            agents.push(AgentChoice {
                name: "Codex".into(),
                detail: "Installed CLI · adapter downloaded with npx on first launch".into(),
                command: vec![
                    npx.clone(),
                    "-y".into(),
                    "--prefer-offline".into(),
                    "@agentclientprotocol/codex-acp".into(),
                ],
            });
        }
        if executable("claude") && !agents.iter().any(|agent| agent.name == "Claude") {
            agents.push(AgentChoice {
                name: "Claude".into(),
                detail: "Installed CLI · adapter downloaded with npx on first launch".into(),
                command: vec![
                    npx,
                    "-y".into(),
                    "--prefer-offline".into(),
                    "@agentclientprotocol/claude-agent-acp".into(),
                ],
            });
        }
    }
    agents
}

pub fn known_agent(command: &[String]) -> Option<KnownAgent> {
    let (program, args) = command.split_first()?;
    let packages = args.iter().filter(|arg| arg.starts_with('@'));
    std::iter::once(program).chain(packages).find_map(|part| {
        let name = Path::new(part).file_name()?.to_str()?.to_ascii_lowercase();
        [
            ("codex", KnownAgent::Codex),
            ("claude", KnownAgent::Claude),
            ("gemini", KnownAgent::GeminiCli),
            ("opencode", KnownAgent::OpenCode),
        ]
        .into_iter()
        .find_map(|(needle, agent)| name.contains(needle).then_some(agent))
    })
}

pub fn agent_name(agent: KnownAgent) -> &'static str {
    match agent {
        KnownAgent::Codex => "Codex",
        KnownAgent::Claude => "Claude",
        KnownAgent::GeminiCli => "Gemini CLI",
        KnownAgent::OpenCode => "OpenCode",
    }
}

fn agent_args(agent: KnownAgent) -> &'static [&'static str] {
    match agent {
        KnownAgent::Codex | KnownAgent::Claude => &[],
        KnownAgent::GeminiCli => &["--acp"],
        KnownAgent::OpenCode => &["acp"],
    }
}

pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let query = query.to_lowercase();
    let candidate = candidate.to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let mut chars = candidate.char_indices();
    let mut last = None;
    let mut score = 0;
    for needle in query.chars() {
        let (offset, _) = chars.find(|(_, character)| *character == needle)?;
        score += if offset == 0
            || candidate[..offset]
                .chars()
                .last()
                .is_some_and(|character| matches!(character, '/' | '_' | '-' | ' '))
        {
            12
        } else {
            2
        };
        if last.is_some_and(|previous| previous + 1 == offset) {
            score += 5;
        }
        last = Some(offset);
    }
    Some(score - (candidate.len() as i32 / 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn finds_and_runs_batch_commands_without_an_extension() {
        let directory = tempfile::tempdir().unwrap();
        let command = directory.path().join("agentaps-probe");
        let batch = command.with_extension("cmd");
        fs::write(&batch, "@echo off\r\nexit /b 0\r\n").unwrap();

        let found = find_executable(command.to_str().unwrap()).unwrap();
        assert_eq!(found.canonicalize().unwrap(), batch.canonicalize().unwrap());
        assert!(
            std::process::Command::new(found)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn recognizes_known_agents_from_commands() {
        let command = |parts: &[&str]| {
            parts
                .iter()
                .map(|part| (*part).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            known_agent(&command(&["codex-acp"])),
            Some(KnownAgent::Codex)
        );
        assert_eq!(
            known_agent(&command(&[
                "npx",
                "-y",
                "@agentclientprotocol/claude-agent-acp"
            ])),
            Some(KnownAgent::Claude)
        );
        assert_eq!(
            known_agent(&command(&["/opt/gemini", "--acp"])),
            Some(KnownAgent::GeminiCli)
        );
        assert_eq!(
            known_agent(&command(&["/usr/bin/opencode", "acp"])),
            Some(KnownAgent::OpenCode)
        );
        assert_eq!(known_agent(&command(&["my-agent", "--stdio"])), None);
        assert_eq!(
            known_agent(&command(&["my-agent", "--model", "claude-sonnet-4"])),
            None
        );
    }

    #[test]
    fn configured_executables_replace_path_lookup() {
        let agents = installed_agents(&BTreeMap::from([
            (KnownAgent::GeminiCli, "/opt/gemini".into()),
            (KnownAgent::OpenCode, "/opt/opencode".into()),
        ]));
        for (name, command) in [
            ("Gemini CLI", ["/opt/gemini", "--acp"]),
            ("OpenCode", ["/opt/opencode", "acp"]),
        ] {
            let matching: Vec<_> = agents.iter().filter(|agent| agent.name == name).collect();
            assert_eq!(matching.len(), 1, "{name}");
            assert_eq!(matching[0].command, command);
            assert_eq!(matching[0].detail, "Set in settings");
        }
    }

    #[test]
    fn fuzzy_match_accepts_subsequences() {
        assert!(score("abc", "a_b-c").is_some());
        assert!(score("xyz", "dev-terminal").is_none());
    }
}
