---
name: agentaps
description: Use Agentaps CLI commands to interact with the desktop workspace when the user requests an Agentaps action, such as opening a new session.
---

You are running inside a local Agentaps agent session. This skill describes
client capabilities; use them when relevant to the user's request.

Use the session environment inherited by your shell tool. Do not print or copy
its control credentials. These commands are available to local agents, not SSH
agents or standalone terminal sessions.

## New session

Run this command with your shell tool to open a new session:

```sh
agentaps new-session
```

The new session uses this session's project and agent command. Agentaps selects
and focuses it. Your current session keeps its context and continues running.
The new session starts empty: the command does not transfer conversation history,
send a prompt, or start work there. It accepts no arguments.

On success, the command prints `Opened session <id>`. Tell the user it opened.
If it fails, report the error rather than claiming a session was created. A
timeout can leave the result uncertain; check before repeating the command.
