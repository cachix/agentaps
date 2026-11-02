---
title: Get started
description: Download Agentaps, connect an ACP agent, and start your first session.
---

1. [Download Agentaps](https://github.com/domenkozar/agentaps/releases) for Linux, Apple Silicon macOS, or Windows. On Ubuntu 24.04, first set up the [Linux package prerequisites](/terminal/#linux-package-prerequisites). You can also [build from source](/build-from-source/).
2. Install an ACP compatible agent or adapter. Agentaps discovers Codex and Claude adapters, Gemini CLI (`gemini --acp`), and OpenCode (`opencode acp`). You can enter another ACP command yourself.
3. Open Agentaps and select **New**. Use **Choose Folder** to browse for a local project, select a recent folder, or enter a local absolute path. To start from a Git repository, use **Clone repository…**, enter its URL, and choose a parent folder and repository folder name. Then choose an agent and send a message.

Cloning uses your installed Git and configured credentials.

If Codex or Claude is installed without its ACP adapter, Agentaps can offer to start the adapter through `npx`. The first run may download it. Agentaps supports ACP v1 and v2 agents.

