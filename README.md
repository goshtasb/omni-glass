# Omni-Glass

**Stop copy-pasting into ChatGPT.**

You see a Python traceback in your terminal. Today, you select it, copy it, open a browser, paste it into an AI chatbot, read the response, switch back to your terminal, and type the fix. Seven steps. Ten seconds of context-switching.

With Omni-Glass: snip the error, click "Fix Error," paste the fix. Three seconds. You never leave your screen.

![Omni-Glass Demo](docs/assets/demo.gif)

Omni-Glass is an open-source desktop tool for macOS. It sits in your menu bar. When you need it, you click the icon, draw a box around anything on your screen, and get a menu of intelligent actions — powered by your choice of LLM, running locally or in the cloud.

## What it does

**Snip a terminal error —** get an explanation, a fix command, or a GitHub issue created automatically.

**Snip a data table —** export it as a CSV file with a native save dialog.

**Snip foreign documentation —** get an instant translation.

**Snip anything —** copy the extracted text, search the web, or ask the LLM to explain it.

**Don't want to snip?** Click "Type Command" in the menu bar and type what you want in plain English. Same pipeline, no screenshot needed.

## How it works

1. You draw a box on your screen
2. OCR runs locally on your device (Apple Vision on macOS, Windows OCR on Windows). **No screenshots leave your machine.**
3. The extracted text goes to an LLM (Claude Haiku, Gemini Flash, or a local model via llama.cpp)
4. You get a menu of contextual actions in under 1 second
5. Click an action. It executes.

That's the entire product. Everything else is about making it extensible and secure.

## Built-in actions

| Action | What it does |
|--------|-------------|
| Explain Error | Explains what went wrong and why |
| Fix Error | Returns a shell command or corrected code — you choose to run it |
| Export CSV | Extracts tabular data into a CSV file |
| Explain This | Plain-English explanation of whatever you snipped |
| Copy Text | Copies OCR-extracted text to clipboard |
| Search Web | Opens a browser search for the snipped content |
| Quick Translate | Translates snipped text to your preferred language |

## Extend it with plugins

Omni-Glass supports the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/). Any MCP server that runs over `stdio` can add actions to your menu.

**Example: the GitHub Issues plugin.** Snip a terminal error. The action menu now includes "Create GitHub Issue." Click it. Omni-Glass extracts the error, generates a title and description, and creates the issue in your repo via the GitHub API. You get a link to the new issue.

Build your own plugin in 10 minutes:

```bash
git clone https://github.com/goshtasb/omni-glass-plugin-template.git
cd omni-glass-plugin-template
# Edit index.js — add your tool logic
# Copy to ~/.config/omni-glass/plugins/your-plugin/
# Restart Omni-Glass — your action appears in the menu
```

Read the [Plugin Developer Guide](docs/plugin-guide.md) for the full walkthrough.

## What you could build

Omni-Glass sees your screen and has an MCP plugin system. Anything you can do with text input and an API, you can trigger from a screen snip or a typed command:

- **Snip a Slack message —** create a Linear, Jira, or Asana ticket with the context already filled in
- **Snip a design mockup —** generate the Tailwind CSS that matches it
- **Snip a meeting invite —** check your Google Calendar for conflicts
- **Snip a SQL error —** query your database schema and suggest the fix
- **Snip a log file —** send it to Datadog or Grafana as a tagged event
- **Snip a code snippet —** run it in a sandbox and return the output
- **Snip a receipt —** extract the total and log it to your expense tracker
- **Snip an API response —** generate the TypeScript types automatically
- **Snip a competitor's UI —** diff it against your own product's screenshots

Each of these is an MCP server with one tool. Most are under 100 lines of code. The [plugin template](https://github.com/goshtasb/omni-glass-plugin-template) gives you the boilerplate — you just write the API call.

## Security

Every plugin runs inside a kernel-level macOS sandbox (`sandbox-exec`).

- **Your home directory is walled off.** Plugins cannot read anything under `/Users/` unless you explicitly approve a specific path.
- **API keys are stripped.** Environment variables are filtered before a plugin process starts. Your `ANTHROPIC_API_KEY`, `AWS_SECRET_ACCESS_KEY`, and other secrets are invisible to plugins.
- **Shell commands require confirmation.** If any action wants to run a command, you see it first and click "Run" or "Cancel."
- **PII is redacted.** Credit card numbers, SSNs, and API keys in your snipped text are redacted before being sent to a cloud LLM.

When you install a plugin, a permission dialog shows exactly what it can access. You approve or deny. No silent escalation.

## Bring your own key (BYOK)

There are no Omni-Glass servers. Your API key talks directly to Anthropic or Google. We never see your data, your prompts, or your API usage.

| Provider | Type | Speed |
|----------|------|-------|
| Claude Haiku | Cloud API | ~3s full pipeline |
| Gemini Flash | Cloud API | Built, not yet benchmarked |
| Qwen-2.5-3B | Local (llama.cpp) | ~8-15s, no internet needed |

Switch providers anytime in Settings. Local mode means zero cloud dependency — your screen content never leaves your machine.

## Quick start

Requires: macOS 12+, Rust, Node.js 18+, an API key (Anthropic or Google), or use local mode with no key.

```bash
git clone https://github.com/goshtasb/omni-glass.git
cd omni-glass
npm install
npm run tauri dev
```

On first launch:
1. Click the Omni-Glass icon in your menu bar
2. Go to Settings — paste your API key (or download a local model)
3. Click "Snip Screen" — draw a box around something — see the action menu

## Project status

Omni-Glass is in active development. Here's what works today and what's coming:

| Feature | Status |
|---------|--------|
| macOS snip — OCR — action menu | Working |
| 7 built-in actions | Working |
| MCP plugin system with sandbox | Working |
| Text launcher (type commands) | Working |
| Local LLM (Qwen-2.5 via llama.cpp) | Built, testing |
| GitHub Issues plugin | Working |
| Windows support | Code written, untested on hardware |
| Linux support | Planned |
| Plugin registry (in-app browse/install) | Planned |
| UI element detection (click buttons, fill forms) | Planned |

## Contributing

We're looking for help with:

- **Breaking the sandbox.** Try to escape the macOS `sandbox-exec` profile. If you can read `~/.ssh/id_rsa` from a plugin, that's a critical bug.
- **Windows testing.** The code compiles in CI but hasn't been tested on real hardware.
- **Linux port.** Tesseract OCR integration, Bubblewrap sandbox, tray icon on Wayland.
- **Plugins.** Build something useful and share it.

## License

MIT
