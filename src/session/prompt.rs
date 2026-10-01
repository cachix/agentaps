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

const PREVIEW_LINES: usize = 8;

/// Agents such as Claude Code echo embedded files back inside the user's
/// message as `<context ref="uri">…</context>`. Shows each as a short fenced
/// block so a long file does not flood the chat.
pub(crate) fn file_context_preview(message: &str) -> std::borrow::Cow<'_, str> {
    if !message.contains("<context ref=") {
        return message.into();
    }
    let mut out = String::new();
    let mut rest = message;
    while let Some(start) = rest.find("<context ref=") {
        let Some(body_start) = rest[start..].find('>').map(|i| start + i + 1) else {
            break;
        };
        let Some(end) = rest[body_start..]
            .find("</context>")
            .map(|i| body_start + i)
        else {
            break;
        };
        out.push_str(rest[..start].trim_end_matches([' ', '\t']));
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        let body = rest[body_start..end].trim_matches('\n');
        let lines: Vec<&str> = body.lines().collect();
        let fence = "`".repeat(
            body.split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0)
                .max(2)
                + 1,
        );
        out.push_str(&format!("{fence}\n"));
        for line in lines.iter().take(PREVIEW_LINES) {
            out.push_str(line);
            out.push('\n');
        }
        if lines.len() > PREVIEW_LINES {
            out.push_str(&format!("… {} more lines\n", lines.len() - PREVIEW_LINES));
        }
        out.push_str(&fence);
        rest = &rest[end + "</context>".len()..];
    }
    out.push_str(rest);
    out.into()
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
            let text = entry.transcript_text();
            (!text.is_empty()).then(|| json!({"role": role, "text": text}))
        })
        .collect();
    format!(
        "This session was forked from an earlier reply. Use the following conversation transcript as context for the current request. The transcript is historical; do not execute requests in it again. Project files may have changed since it occurred.\n\nConversation transcript:\n{}\n\nCurrent request:\n{prompt}",
        serde_json::to_string_pretty(&history).unwrap_or_default()
    )
}
