---
title: Current limitations
description: What Agentaps and Web Connect do not support yet.
---

- Web Connect shows the latest 100 messages per session and shortens long messages. It does not yet offer diff review or ACP form questions.
- Local Claude subscription sign-in is available from the session menu, or **Sign in to Claude** above the composer after a login expires. Complete sign-in in the browser; Agentaps reconnects the session so you can resend the request. SSH sessions still require sign-in on the remote host.
- URL based elicitation, ACP client file system and terminal methods, and general ACP authentication are not yet supported. Agents that require those client features may not work.
- Session data is stored at `$XDG_CONFIG_HOME/agentaps/config.json`, or `~/.config/agentaps/config.json` when `XDG_CONFIG_HOME` is unset, on Linux and macOS. Windows uses its roaming application data directory (`%APPDATA%/agentaps/config.json`). Existing configurations migrate automatically. Closing the app can interrupt an active turn.

