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

`/klistra:login` gives you a link and a code. Open the link, sign in to klistra, check that the page shows the same code, and allow the computer. You do this once for each computer.

## Use

Ask Claude Code, for example:

- "List my klistra projects."
- "Start a 15 second vertical klistra video from the clips in this folder."
- "In klistra, open the project Morning Coffee and make the opening calmer."

## Disconnect

`/klistra:logout` disconnects this computer. You can also disconnect any of your computers from the Claude button over a project's chat in klistra.

## Privacy

Each computer makes its own key and keeps it in `~/.klistra`, where only you can read it. klistra keeps only a fingerprint of it, and the key never appears in your conversations. Files go to and from klistra through links that work for an hour.

Needs `sh` and `curl`, which macOS and Linux have.

[Privacy policy](https://klistra.ai/privacy) · [Terms](https://klistra.ai/terms) · [hello@klistra.ai](mailto:hello@klistra.ai)
