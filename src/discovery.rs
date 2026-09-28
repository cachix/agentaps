use std::{
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

pub fn installed_agents() -> Vec<AgentChoice> {
    let mut agents = Vec::new();
    let mut add = |name: &str, detail: &str, command: &[&str]| {
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
    fn fuzzy_match_accepts_subsequences() {
        assert!(score("abc", "a_b-c").is_some());
        assert!(score("xyz", "dev-terminal").is_none());
    }
}
