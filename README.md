# Omni-Glass

## Why I Built This

Claude Desktop can take screenshots now. So can ChatGPT. You snip your screen, the AI looks at it, and writes you a nice explanation of what went wrong.

Then you still have to fix it yourself.

I kept thinking: the AI clearly knows the answer. It just told me `pip install pandas` will fix this. Why am I the one typing it? Why can't it just... do it?

So I built Omni-Glass. You snip your screen, and instead of getting a chat response, you get a button that says "Fix Error." You click it. It runs the command. Done.

That's the core idea: **AI that acts on what it sees, not just talks about it.**

![Omni-Glass Demo](docs/assets/demo.gif)

## How It's Different From Claude Desktop

I tested both side by side. Here's what actually happened:

| I tried... | Claude Desktop | Omni-Glass |
|-----------|---------------|------------|
| "How much RAM is Chrome using?" | Told me how to open Activity Monitor | Ran the command and showed me the answer |
| Export a data table as CSV | Gave me a messy blob of text | Opened a native save dialog with a clean CSV |
| Fix a Python error | Explained the fix in a paragraph | Generated `pip install pandas`, asked me to confirm, ran it |
| Create a GitHub issue from a bug | Wrote a draft I'd need to copy-paste | Created the issue on GitHub and gave me the link |

Claude reads your screen and talks about it. Omni-Glass reads your screen and does something about it.

### The security difference nobody talks about

Claude Desktop runs MCP plugins with your full user permissions. If a plugin goes rogue — or if a prompt injection hits — it has access to your SSH keys, your `.env` files, your browser cookies. Everything.

Omni-Glass sandboxes every plugin at the macOS kernel level. Your entire `/Users/` directory is walled off. A plugin physically cannot read your home folder unless you explicitly approved a specific path. Environment variables are filtered. Shell commands require your confirmation.

I built this because I want to run community plugins without worrying about what they can access.

## Quick Start

Requires: macOS 12+, Rust, Node.js 18+

```bash
git clone https://github.com/goshtasb/omniglass.git
cd omniglass
npm install
npm run tauri dev
```

1. Click the Omni-Glass icon in your menu bar
2. Settings → paste your Anthropic or Google API key (or download a local model — no API key needed)
3. Click "Snip Screen" → draw a box → see the action menu

> **No API key?** Omni-Glass runs Qwen-2.5-3B locally via llama.cpp. Full pipeline in ~6 seconds, nothing leaves your machine. Select "Local" in Settings and download the model.

## See It Work

**Snip a Python traceback →** Omni-Glass generates `pip install pandas` and shows a "Run" button. One click, it executes.

**Snip a data table →** A native save dialog opens. Your CSV is ready.

**Snip a Slack bug report →** A GitHub issue is created in your repo with the title and description filled in.

**Snip Japanese documentation →** The English translation is on your clipboard.

**Type a command →** Click "Type Command" in the menu bar. "How much disk space do I have?" It runs `df -h` and shows you the answer. No snipping needed.

## Built-in Actions

| Action | What happens when you click it |
|--------|-------------------------------|
| Fix Error | Generates a shell command or code fix. You confirm, it runs. |
| Explain Error | Plain-English explanation of the error |
| Export CSV | Extracts table data into a CSV with a native save dialog |
| Explain This | Explains whatever you snipped |
| Copy Text | OCR-extracted text → clipboard |
| Search Web | Opens a browser search |
| Quick Translate | Translates and copies to clipboard |

## Build Your Own Actions

This is where it gets interesting. Omni-Glass is built on [MCP (Model Context Protocol)](https://modelcontextprotocol.io/). If you can write a Node.js or Python script that takes JSON in and puts JSON out, you can add any action to the menu.

**You don't write prompt engineering.** Omni-Glass handles the hard part — it reads the raw screen text and automatically generates the structured JSON arguments your tool expects. You just write the API call.

**What you could build (each is a single MCP server, most under 100 lines):**

- **Snip a Slack message →** create a Jira/Linear/Asana ticket with context filled in
- **Snip a design mockup →** generate the Tailwind CSS
- **Snip a SQL error →** query your database schema, suggest the fix
- **Snip a log file →** send it to Datadog or Grafana as a tagged event
- **Snip a receipt →** extract the total, log it to your expense tracker
- **Snip an API response →** generate TypeScript types
- **Snip a meeting invite →** check your Google Calendar for conflicts

```bash
# Start building a plugin
# 1. Look at the GitHub Issues plugin in the repo as a reference
# 2. Create a folder in ~/.config/omni-glass/plugins/your-plugin/
# 3. Add an omni-glass.plugin.json manifest
# 4. Write your index.js MCP server
# 5. Restart Omni-Glass — your action appears in the menu
```

Read the [Plugin Developer Guide](docs/plugin-guide.md) for the full walkthrough.

## Contributing

**Don't just read the code. Break it.**

The most valuable thing you can do is try to escape the sandbox. Every plugin runs inside a macOS `sandbox-exec` profile that walls off your home directory. If you can read `~/.ssh/id_rsa` from inside a plugin process, that's a critical security bug and I want to know immediately.

**Build a plugin.** Pick any API you use daily and make it an Omni-Glass action. If it's useful to you, it's useful to others. Open a PR or share it in Discord.

**Plugin ideas we'd love to see (good first issues):**

| Plugin | Difficulty | What it does |
|--------|-----------|-------------|
| Slack Webhook | Easy | Snip anything → send to a Slack channel |
| Jira/Linear Ticket | Easy | Snip a bug → create a ticket |
| Notion Clipper | Medium | Snip content → save to a Notion page |
| Terminal Command | Easy | Snip an error → suggest and run the fix command |
| AWS Console Helper | Medium | Snip an AWS error → look up the service docs |
| Datadog Event | Easy | Snip a log → send as a Datadog event |

**Port to other platforms.** Windows code compiles in CI but has never been tested on real hardware. Linux needs Tesseract OCR integration and Bubblewrap sandbox. Both are meaningful contributions.

## Architecture

One paragraph, not a lecture:

You snip your screen → Apple Vision OCR extracts text locally (no images leave your machine) → the text goes to an LLM (Claude Haiku, Gemini Flash, or Qwen-2.5 locally via llama.cpp) → the LLM classifies the content and streams a menu of actions in under 1 second → you click an action → it executes through the built-in handler or a sandboxed MCP plugin. Two inputs (snip or type), one pipeline, same plugins.

## Security Model

| Layer | What it does |
|-------|-------------|
| **macOS sandbox-exec** | Kernel-level isolation. Plugins cannot read `/Users/` unless you approved a specific path. |
| **Environment filtering** | API keys and secrets are stripped before plugin processes start. |
| **Command confirmation** | Every shell command shows in the UI. You click "Run" or "Cancel." |
| **PII redaction** | Credit card numbers, SSNs, and API keys are scrubbed before text goes to a cloud LLM. |
| **Permission prompt** | On first install, a dialog shows exactly what the plugin can access. You approve or deny. |

## Providers

No Omni-Glass servers. Your key, your data, direct to the provider.

| Provider | Type | Pipeline Speed |
|----------|------|---------------|
| Claude Haiku | Cloud | ~3s |
| Gemini Flash | Cloud | Built, benchmarking soon |
| Qwen-2.5-3B | Local (llama.cpp) | ~6s, zero cloud dependency |

## Status

Omni-Glass is in active development. It works today on macOS. Here's where things stand:

| Feature | Status |
|---------|--------|
| Screen snip → OCR → action menu | ✅ Working |
| 7 built-in actions | ✅ Working |
| MCP plugin system + sandbox | ✅ Working |
| Text launcher | ✅ Working |
| Local LLM (Qwen-2.5) | ✅ Working |
| GitHub Issues plugin | ✅ Working |
| Windows | 🔧 Compiles, untested on hardware |
| Linux | 📋 Planned |
| Plugin registry (in-app browse) | 📋 Planned |
| Pre-built .dmg installer | 📋 Coming soon |

## Community

Questions, plugin ideas, or want to show what you built?

→ [Join the Discord](https://discord.gg/YOUR_INVITE_LINK)

## License

MIT
