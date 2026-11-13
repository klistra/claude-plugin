---
name: login
description: Connect this computer to klistra, so that Claude Code can plan the person's klistra projects. Use when the person asks to connect or sign in to klistra, or when a klistra tool says this computer is not connected.
allowed-tools: Bash(klistra login) Bash(klistra login --wait)
---

Connect this computer to the person's klistra account:

1. Run `klistra login`. It prints a link and a code. If it says the computer is already connected, tell the person to which team, and stop.
2. Show the person the link and the code, in your reply, and ask them to open the link, sign in to klistra, check that the page shows the same code, and allow the computer.
3. Run `klistra login --wait`. It waits up to 100 seconds and says when the computer is connected. While it says it is still waiting, run it again, up to three times; then ask the person to tell you when they have allowed it.
4. Once connected, say which team Claude Code can plan in now, and that they can ask you to list their klistra projects, start a new one, or work on one by name.

Planning with Claude Code is part of klistra Pro: if the page says the team needs Pro, tell the person that.
