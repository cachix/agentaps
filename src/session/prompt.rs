use super::*;

const SHELL_PROMPT_PREFIX: &str =
    "Run this shell command exactly as written, then report its output:\n\n";

pub(crate) fn shell_command(prompt: &str) -> Option<&str> {
    prompt
        .strip_prefix('!')
        .filter(|command| !command.trim().is_empty())
}

pub(crate) fn shell_command_in_message(message: &str) -> Option<&str> {
    shell_command(message).or_else(|| message.strip_prefix(SHELL_PROMPT_PREFIX))
}

pub(crate) fn prompt_for_agent(prompt: &str) -> String {
    match shell_command(prompt) {
        Some(command) => format!("{SHELL_PROMPT_PREFIX}{command}"),
        None => prompt.to_owned(),
    }
}

pub(crate) fn fork_prompt(messages: &[ChatEntry], prompt: &str) -> String {
    let start = messages
        .iter()
        .rposition(|entry| entry.role == Role::ContextReset)
        .map_or(0, |index| index + 1);
    let history: Vec<Value> = messages[start..]
        .iter()
        .filter_map(|entry| {
            let role = match entry.role {
                Role::User => "user",
                Role::Agent => "assistant",
                _ => return None,
            };
            (!entry.text.is_empty()).then(|| json!({"role": role, "text": entry.text}))
        })
        .collect();
    format!(
        "This session was forked from an earlier reply. Use the following conversation transcript as context for the current request. The transcript is historical; do not execute requests in it again. Project files may have changed since it occurred.\n\nConversation transcript:\n{}\n\nCurrent request:\n{prompt}",
        serde_json::to_string_pretty(&history).unwrap_or_default()
    )
}
