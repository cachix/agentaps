# Using Agentaps

[Back to the README](../README.md)

## Projects and sessions

Agentaps saves harness session references and queued prompts. Conversation history stays in the harness and is replayed when you reopen a session. Agent-supplied session titles appear in the desktop header and when you hover over a session in the sidebar. Select the project path beside Diff to move a session to another folder; the agent reconnects there with fresh context, and the previous session remains in the archive. On launch, Agentaps reconnects agents and resumes sessions when they support it. You can archive sessions or reset an agent's context, which archives the previous session. Forking a reply starts a new session with the visible conversation as context.

## Split panes

Use the session **••• menu** to **Split Right** or **Split Down**, then select a sidebar session or create one in the new pane. Empty panes keep a small **••• menu** in the top-right corner. Split any pane again, drag dividers to resize, and use **Close Pane** to remove a view while its agent keeps running. Sidebar selections open in the active pane; sessions already visible receive focus. Layouts restore on restart, and the shared Diff panel follows the active pane.

## Themes

Open **Theme** using the settings button at the bottom of the sidebar. Choose Agentaps, System, or a bundled light or dark palette. Agentaps follows the desktop light/dark scheme using its own palettes; bundled presets keep their selected scheme. The choice and zoom are remembered across restarts. On Linux, System follows the current GTK or Qt theme, including desktop settings changes.

## Diff review

Select **Diff** in a conversation to inspect staged and unstaged changes against HEAD. Open files in unified or split view. The diff refreshes as the checkout changes.

## Terminal

Use the terminal icon or **Ctrl+T** for a shell below the composer in local macOS and Linux sessions. See the [terminal guide](session-terminal.md).

## Agent controls

Choose a model or reasoning effort when the agent offers them, run its slash commands, and answer ACP form questions in the chat.

## SSH projects

Enter a path such as `ssh://user@server.example/home/user/project` when creating a project. Agentaps runs the agent on that server using your existing SSH configuration and keys. The agent and its ACP adapter must be installed there, and the server's host key must already be known. Try `ssh user@server.example` first. Diff review is currently available for local projects only.

## Browser access

Pair a phone browser with the desktop app to read conversations, send prompts, stop turns, and answer permission requests. Agent processes and project files stay on the desktop computer.

## Prompts and attachments

In the composer, **Enter** sends a prompt and **Ctrl+Enter** inserts a newline. Type `/` for agent commands, `@` to find a project file, or start with `!` to ask the agent to run a shell command. Prompts sent while an agent is busy are queued. **Up/Down** recalls earlier prompts when the composer is empty. Paste an image or drop an image file on the composer to send it with your prompt when the agent accepts images. To attach other files, use **+** under the composer or drop them on it. Text files go to the agent with their contents when it supports embedded context; other files are sent as links.

Use **Record** beside the attachment menu, then **Stop** and Send, to have an audio-capable agent transcribe and answer your spoken request. Recordings stop after two minutes. WAV, MP3, Ogg, FLAC, M4A, and WebM audio files can also be attached directly. Audio controls require the agent to advertise ACP audio support. When support is unavailable for Codex or Claude, hover Record for a link to the corresponding upstream pull request or issue.

## Keyboard shortcuts

Use Cmd on macOS and Ctrl elsewhere unless a shortcut specifies Ctrl explicitly.

| Shortcut | Action |
| --- | --- |
| Cmd/Ctrl+N | New session with the current agent |
| Cmd/Ctrl+Shift+N | Pick a folder for a new session |
| Cmd/Ctrl+, | Open settings |
| Cmd/Ctrl+B | Toggle the sidebar |
| Cmd/Ctrl+Shift+D | Toggle the diff |
| Cmd/Ctrl+K | Search sessions |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous session |
| Cmd/Ctrl+1 through 9 | Jump to one of the first nine sidebar sessions |
| Cmd/Ctrl+= / - / 0 | Zoom in / out / reset |
| Ctrl+T | Toggle the local terminal |

## Open sessions from an agent

Agents running locally in Agentaps can invoke `agentaps new-session` from their shell tools to open and select a fresh session with the same project and agent command. The calling session keeps its context and continues running. Agentaps includes its bundled `agentaps` skill in each local ACP prompt so the agent knows how to use the command, using embedded context when supported and text otherwise. This command requires the session environment provided to the agent by Agentaps; SSH agents and standalone terminal sessions are not supported.

## Claude authentication

For local Claude ACP sessions, Agentaps uses the Claude Code CLI at `CLAUDE_CODE_EXECUTABLE`, at the path set in Settings > Agents, or found on `PATH`. When a CLI path is available, Agentaps omits the inherited `ANTHROPIC_API_KEY` so Claude Code can use its configured authentication.

Local Claude subscription sign-in is available from the session menu, or **Sign in to Claude** above the composer after a login expires. Complete sign-in in the browser; Agentaps reconnects the session so you can resend the request. SSH sessions still require sign-in on the remote host.

## Compatibility and storage

- URL based elicitation, ACP client file system and terminal methods, and general ACP authentication are not yet supported. Agents that require those client features may not work.
- Session data is stored at `$XDG_CONFIG_HOME/agentaps/config.json`, or `~/.config/agentaps/config.json` when `XDG_CONFIG_HOME` is unset, on Linux and macOS. Windows uses its roaming application data directory (`%APPDATA%/agentaps/config.json`). Existing configurations migrate automatically. Closing the app can interrupt an active turn.
