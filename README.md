# klistra for Claude Code

Plan short videos and pictures in your [klistra](https://klistra.ai) projects from Claude Code. With this plugin, Claude Code can list your projects and start new ones, write and edit a project's plan, bring in files from your computer, build, and save the renders where you want them. Each edit shows up in the klistra studio as it is made, and can be undone there.

Planning from Claude Code is part of klistra Pro.

## Set up

In klistra, open a project and click the Claude button over its chat. Copy the setup prompt there and paste it into Claude Code: it installs the plugin and gives you a link to allow the computer.

Or set it up by hand, in Claude Code:

```
/plugin marketplace add klistra/claude-plugin
/plugin install klistra@klistra
/reload-plugins
/klistra:login
```

`/klistra:login` gives you a link and a code. Open the link on any device, your phone too, so a computer you work on over SSH needs no browser of its own. Sign in to klistra, check that the page shows the same code, and allow the computer. You do this once for each computer.

## Use

Ask Claude Code, for example:

- "List my klistra projects."
- "Start a 15 second vertical klistra video from the clips in this folder."
- "In klistra, open the project Morning Coffee and make the opening calmer."

When Claude Code works in a project, it connects to one of the project's chats in the studio. You see there that it is connected and what it changes, and what you write in that chat, or say about something you point at in a render, goes to Claude Code in place of klistra's planner. Disconnect it there to talk to the planner again.

## Disconnect

`/klistra:logout` disconnects this computer. You can also disconnect any of your computers from the Claude button over a project's chat in klistra.

## Privacy

Each computer makes its own key and keeps it in `~/.klistra`, where only you can read it. klistra keeps only a fingerprint of it, and the key never appears in your conversations. Files go to and from klistra through links that work for an hour.

Runs on macOS, Linux and Windows (x64). Sending and saving files uses `curl`, which all three have.

## How it works

The plugin carries a small program, `klistra`, built for each of those systems from the Rust source in [runtime-src](runtime-src). Claude Code starts it as the plugin's MCP server: it passes each call on to klistra over HTTPS with this computer's key, so Claude Code never handles the key itself. While Claude Code is connected to a chat, the program waits at klistra for your messages there and puts each into the Claude Code session, which takes it up even when it is idle. `klistra login` connects the computer, and `klistra status` and `klistra logout` do what they say.

[Privacy policy](https://klistra.ai/privacy) · [Terms](https://klistra.ai/terms) · [hello@klistra.ai](mailto:hello@klistra.ai)

The plugin is MIT licensed; see [LICENSE](LICENSE). klistra itself is a service, under its [terms](https://klistra.ai/terms).
